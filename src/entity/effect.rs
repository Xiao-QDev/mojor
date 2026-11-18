// 生物效果(药水) 照搬 MCPE 0.14.1 / ITX Effect 与 MobEffectPacket
// 服务端只维护 duration/amplifier 并驱动血/饥饿变化 视觉由 MobEffectPacket 交给客户端
use std::collections::HashMap;

// 效果 ID 与 ITX entity/Effect.php 一致
pub const SPEED: u8 = 1;
pub const SLOWNESS: u8 = 2;
pub const HASTE: u8 = 3;
pub const STRENGTH: u8 = 5;
pub const JUMP: u8 = 8;
pub const NAUSEA: u8 = 9;
pub const REGENERATION: u8 = 10;
pub const FIRE_RESISTANCE: u8 = 12;
pub const WATER_BREATHING: u8 = 13;
pub const INVISIBILITY: u8 = 14;
pub const BLINDNESS: u8 = 15;
pub const NIGHT_VISION: u8 = 16;
pub const HUNGER: u8 = 17;
pub const WEAKNESS: u8 = 18;
pub const POISON: u8 = 19;
pub const WITHER: u8 = 20;
pub const HEALTH_BOOST: u8 = 21;
pub const ABSORPTION: u8 = 22;
pub const SATURATION: u8 = 23;
pub const LEVITATION: u8 = 24;
pub const FATAL_POISON: u8 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveEffect {
    pub amplifier: u8,
    pub duration: i32, // 剩余 tick
}

impl ActiveEffect {
    pub fn new(amplifier: u8, duration: i32) -> Self {
        Self { amplifier, duration }
    }
}

// 活跃效果表 effect_id -> 效果
pub type Effects = HashMap<u8, ActiveEffect>;

// 原版药水效果间隔: 每 tick 的周期 数字越小越快
// Regeneration: 每 (50 >> amplifier) tick 回 1 血 (ITX/原版)
// Poison/Wither: 每 (25 >> amplifier) tick 扣 1 血 但不会致死(留 1)
fn regen_period(amplifier: u8) -> u32 {
    50u32 >> amplifier.min(4)
}
fn damage_period(amplifier: u8) -> u32 {
    25u32 >> amplifier.min(4)
}

// 推进一个 tick 返回 (新的血量, 新的饥饿, 该发给客户端的效果包列表: (id, 剩余时长, 是否移除))
// tick_counter 是玩家全局 tick 计数 用取模判断该不该结算
pub fn tick_effects(
    effects: &mut Effects,
    health: i32,
    hunger: f32,
    tick_counter: u32,
) -> (i32, f32, Vec<(u8, i32, bool)>) {
    let mut health = health;
    let mut hunger = hunger;
    let mut packets = Vec::new();
    let mut expired = Vec::new();

    for (&id, effect) in effects.iter_mut() {
        effect.duration -= 1;
        if effect.duration <= 0 {
            expired.push(id);
            packets.push((id, 0, true));
            continue;
        }

        // 周期性结算 只对增益/减益血量的效果
        if id == REGENERATION {
            if tick_counter % regen_period(effect.amplifier) == 0 && health < 20 {
                health = (health + 1).min(20);
            }
        } else if id == POISON || id == WITHER {
            if tick_counter % damage_period(effect.amplifier) == 0 && health > 1 {
                health -= 1;
            }
        } else if id == SATURATION {
            // 饱和效果持续回饥饿
            if tick_counter % 10 == 0 {
                hunger = (hunger + 1.0).min(20.0);
            }
        } else if id == HUNGER {
            // 饥饿效果加速疲劳
            if tick_counter % 10 == 0 {
                hunger = (hunger - 1.0).max(0.0);
            }
        }
    }

    for id in expired {
        effects.remove(&id);
    }
    (health, hunger, packets)
}

// 药水 damage 值 -> (效果 id, amplifier, 基础时长 tick)
// 非效果药水(水/平凡/笨拙/浓稠) 返回 None
pub fn potion_effect(damage: i16) -> Option<(u8, u8, i32)> {
    let (id, amp, seconds) = match damage {
        34 | 35 | 36 => (REGENERATION, (damage - 34) as u8, if damage == 34 {45} else if damage == 35 {90} else {22}),
        14 | 15 | 16 => (SPEED, (damage - 14) as u8, if damage == 14 {90} else if damage == 15 {180} else {45}),
        12 | 13 => (FIRE_RESISTANCE, 0, if damage == 12 {45} else {90}),
        5 | 6 => (NIGHT_VISION, 0, if damage == 5 {90} else {180}),
        31 | 32 | 33 => (STRENGTH, (damage - 31) as u8, if damage == 31 {45} else if damage == 32 {90} else {22}),
        9 | 10 | 11 => (JUMP, (damage - 9) as u8, if damage == 9 {90} else if damage == 10 {180} else {45}),
        19 | 20 => (WATER_BREATHING, 0, if damage == 19 {45} else {90}),
        7 | 8 => (INVISIBILITY, 0, if damage == 7 {90} else {180}),
        25 | 26 | 27 => (POISON, (damage - 25) as u8, if damage == 25 {45} else if damage == 26 {90} else {22}),
        17 | 18 => (SLOWNESS, 0, if damage == 17 {20} else {40}),
        21 => return Some((0, 0, 0)), // HEALING 瞬间回血 特殊标记
        22 => return Some((1, 0, 0)), // HEALING II
        23 => return Some((2, 0, 0)), // HARMING
        24 => return Some((3, 0, 0)), // HARMING II
        _ => return None,
    };
    Some((id, amp, seconds * 20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potion_maps_effects() {
        assert_eq!(potion_effect(14), Some((SPEED, 0, 1800)));
        assert_eq!(potion_effect(34), Some((REGENERATION, 0, 900)));
        assert_eq!(potion_effect(21), Some((0, 0, 0))); // 治疗
        assert_eq!(potion_effect(0), None); // 水瓶无效果
    }

    #[test]
    fn regeneration_heals_over_time() {
        let mut effects = Effects::new();
        effects.insert(REGENERATION, ActiveEffect::new(0, 100));
        let mut hp = 10;
        // 每 50 tick 回 1 血
        for tick in 1..=100 {
            let (h, _, _) = tick_effects(&mut effects, hp, 20.0, tick);
            hp = h;
        }
        assert!(hp > 10, "再生效果应回血 实际 {hp}");
    }

    #[test]
    fn poison_does_not_kill() {
        let mut effects = Effects::new();
        effects.insert(POISON, ActiveEffect::new(0, 1000));
        let mut hp = 2;
        for tick in 1..=1000 {
            let (h, _, _) = tick_effects(&mut effects, hp, 20.0, tick);
            hp = h;
        }
        assert!(hp >= 1, "中毒不能致死");
    }

    #[test]
    fn effects_expire() {
        let mut effects = Effects::new();
        effects.insert(SPEED, ActiveEffect::new(0, 3));
        for tick in 1..=3 {
            let _ = tick_effects(&mut effects, 20, 20.0, tick);
        }
        assert!(effects.is_empty(), "效果到期应移除");
    }
}
