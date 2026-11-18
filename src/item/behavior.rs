// 物品使用行为 照搬 MCPE 0.14.1 原版各种 *Item 类的 useTimeDepleted / useOn
// region 线程独占调用 玩家状态在这里是权威副本 修改后由 PacketResult 快照回写 global
//
// 客户端吃完食物后上报 EntityEventPacket(event=9 USE_ITEM), 这里据此消耗食物并应用营养.
// 与 ITX Player::eatFoodInHand 及原版 FoodItemComponent::useTimeDepleted 对齐.

use bytes::Bytes;

use crate::entity::PlayerState;
use crate::item::food;
use crate::network::protocol::entity_event::EntityEventPacket;
use crate::network::protocol::update_attributes::{Attribute, UpdateAttributesPacket, HUNGER};
use crate::network::protocol::item::ItemStack;

// 碗类食物吃完后返还的容器 (蘑菇汤/甜菜汤 -> 碗)
fn container_after(id: i16) -> Option<i16> {
    use crate::item::ids;
    match id {
        ids::MUSHROOM_STEW | ids::BEETROOT_SOUP => Some(ids::BOWL),
        _ => None,
    }
}

// 构造饥饿属性包刷新客户端饥饿条
// 与 ITX sendAttributes 一致: entityId=0(客户端自己) 只有 player.hunger 一条
// 饱和/疲劳是服务端内部状态 0.14 客户端属性表不认 player.saturation/exhaustion 不发
pub fn hunger_attributes(state: &PlayerState) -> Bytes {
    UpdateAttributesPacket {
        eid: 0,
        entries: vec![Attribute {
            min: 0.0,
            max: food::MAX_FOOD,
            value: state.hunger,
            name: HUNGER.to_string(),
        }],
    }
    .encode()
}

// 吃完食物后的处理 返回 (给本人的包, 需要广播给视野内所有人的包)
// 只有成功吃掉才返回 Some
pub fn eat(
    state: &mut PlayerState,
    eat_cooldown_ticks: u32,
) -> Option<(Vec<Bytes>, Vec<Bytes>)> {
    if eat_cooldown_ticks > 0 {
        return None;
    }
    if state.health <= 0 {
        return None;
    }

    // 优先用已跟踪的手持槽 否则扫描快捷栏找食物/药水(与 ITX getItemInHand 等价)
    let is_consumable = |item: ItemStack| {
        food::is_food(item.id) || item.id == crate::item::ids::POTION
    };
    let tracked = state.inventory.get(state.held_slot).copied().flatten();
    let held_idx = if tracked.is_some_and(is_consumable) {
        state.held_slot
    } else {
        state
            .hotbar
            .iter()
            .copied()
            .find(|&idx| {
                state
                    .inventory
                    .get(idx as usize)
                    .and_then(|s| *s)
                    .is_some_and(is_consumable)
            })? as usize
    };
    let held = state.inventory.get(held_idx).and_then(|s| *s)?;

    let mut own = Vec::new();
    let mut broad = Vec::new();

    // 药水: 应用效果 + 返还玻璃瓶
    if held.id == crate::item::ids::POTION {
        if let Some((effect_id, amplifier, duration)) = crate::entity::effect::potion_effect(held.damage) {
            match effect_id {
                0 => state.health = (state.health + 4 * (amplifier + 1) as i32).min(20), // 治疗
                2 | 3 => state.health = (state.health - 4 * (amplifier + 1) as i32).max(1), // 伤害
                id => {
                    state.effects.insert(id, crate::entity::effect::ActiveEffect::new(amplifier, duration));
                    let pkt = crate::network::protocol::mob_effect::MobEffectPacket::add(
                        state.eid, id, amplifier, duration,
                    ).encode();
                    own.push(pkt);
                }
            }
            consume_held(state, held_idx);
            state.add_item(ItemStack::new(crate::item::ids::GLASS_BOTTLE, 1));
        } else {
            return None; // 水瓶/无用药水 不消耗
        }
    } else {
        // 普通食物
        let f = food::food_of(held.id)?;
        let (hunger, saturation) = food::apply_eat(state.hunger, state.saturation, f);
        state.hunger = hunger;
        state.saturation = saturation;
        consume_held(state, held_idx);
        if let Some(container) = container_after(held.id) {
            state.add_item(ItemStack::new(container, 1));
        }
    }

    // 吃音效/动画 原版 event=9 USE_ITEM 广播给所有人
    let evt = EntityEventPacket { eid: state.eid, event: crate::network::protocol::entity_event::ENTITY_EVENT_USE_ITEM }.encode();
    own.push(hunger_attributes(state));
    own.push(state.inventory_packet().encode());
    own.push(evt.clone());
    broad.push(evt);
    Some((own, broad))
}

// 扣掉手持槽一个物品 数量为 0 时清空
fn consume_held(state: &mut PlayerState, idx: usize) {
    if let Some(slot) = state.inventory.get_mut(idx).and_then(|s| s.as_mut()) {
        slot.count = slot.count.saturating_sub(1);
        if slot.count == 0 {
            state.inventory[idx] = None;
        }
    }
}

// 每 80 tick 的饥饿回血/饿死判定 返回变化后的 (饥饿, 饱和, 血, 疲劳)
// 与 ITX/原版 HungerAttributeDelegate 一致
pub fn food_tick(hunger: f32, saturation: f32, health: i32, exhaustion: f32, difficulty: i32) -> (f32, f32, i32, f32) {
    let mut hunger = hunger;
    let mut health = health;
    let mut exhaustion = exhaustion;
    if hunger >= food::HEAL_LEVEL && health < 20 {
        health = (health + 1).min(20);
        exhaustion += food::EXHAUSTION_HEAL;
    } else if hunger <= food::STARVE_LEVEL {
        // 简单难度血>10 普通血>1 困难无下限 和平(0)不掉血
        let can_starve = match difficulty {
            0 => false,
            1 => health > 10,
            2 => health > 1,
            _ => health > 0,
        };
        if can_starve {
            health -= 1;
        }
    }
    // 饱和随疲劳衰减
    let (h, s, e) = food::apply_exhaustion(hunger, saturation, exhaustion, 0.0);
    hunger = h;
    (hunger, s, health, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ids;

    fn survival_state() -> PlayerState {
        let mut s = PlayerState::default();
        s.logged_in = true;
        s.spawned = true;
        s.eid = 7;
        s.inventory[9] = Some(ItemStack::new(ids::APPLE, 3));
        s.hunger = 10.0;
        s.saturation = 0.0;
        s
    }

    #[test]
    fn eat_apple_consumes_and_restores_hunger() {
        let mut s = survival_state();
        let (own, broadcast) = eat(&mut s, 0).unwrap();
        assert_eq!(s.hunger, 14.0);
        assert_eq!(s.inventory[9].unwrap().count, 2);
        assert_eq!(own.len(), 3); // 属性 + 背包 + 事件动画
        assert_eq!(broadcast.len(), 1);
    }

    #[test]
    fn eat_cooldown_blocks() {
        let mut s = survival_state();
        assert!(eat(&mut s, 1).is_none());
        assert_eq!(s.hunger, 10.0);
    }

    #[test]
    fn eat_soup_returns_bowl() {
        let mut s = survival_state();
        s.inventory[9] = Some(ItemStack::new(ids::MUSHROOM_STEW, 1));
        eat(&mut s, 0).unwrap();
        // 背包里应多出一个碗
        assert!(s.inventory.iter().flatten().any(|i| i.id == ids::BOWL));
    }

    #[test]
    fn food_tick_heals_and_starves() {
        // 高饥饿回血
        let (_, _, hp, _) = food_tick(19.0, 5.0, 10, 0.0, 2);
        assert_eq!(hp, 11);
        // 饿死(普通难度血>1)
        let (_, _, hp, _) = food_tick(0.0, 0.0, 10, 0.0, 2);
        assert_eq!(hp, 9);
        // 和平不掉血
        let (_, _, hp, _) = food_tick(0.0, 0.0, 10, 0.0, 0);
        assert_eq!(hp, 10);
    }
}
