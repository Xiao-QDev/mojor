// 食物数据 照搬 MCPE 0.14.1 原版 items.json 的 food 段 (ItemInitItemsJson.h)
// 原版用新 ID(apple=4...) 这里按本服协议用的 Nukkit legacy ID(apple=260...) 建表
// 营养 nutrition / 饱和度倍率 saturation / 是否肉类 与 FoodItemComponent 一一对应
//
// 原版 Player::eat(nutrition, satModifier):
//   hunger += nutrition
//   saturation = min(saturation + 2*satModifier*nutrition, hunger)
// 饥饿衰减/回血/饿死 见 HungerAttributeDelegate + FoodConstants:
//   MAX_FOOD=20 HEAL_LEVEL=18 STARVE_LEVEL=0 HEALTH_TICK_COUNT=80
//   START_SATURATION=5 MAX_EXHAUSTION=4 EXHAUSTION_HEAL=3
//
// 注: 腐肉/蜘蛛眼/毒马铃薯的金苹果中毒等 MobEffect 属批次4效果系统 这里只做基础营养

use super::ids;

// 饱食度上限 原版 FoodConstants::MAX_FOOD
pub const MAX_FOOD: f32 = 20.0;
pub const START_SATURATION: f32 = 5.0;
pub const MAX_EXHAUSTION: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct Food {
    pub nutrition: i32,
    pub saturation: f32,
    pub meat: bool,
}

const fn food(nutrition: i32, saturation: f32, meat: bool) -> Food {
    Food { nutrition, saturation, meat }
}

// legacy ID -> 原版食物属性 一一对应 items.json 的 32 种食物
pub fn food_of(id: i16) -> Option<Food> {
    let f = match id {
        ids::APPLE => food(4, 0.3, false),
        ids::GOLDEN_APPLE => food(4, 1.2, false),
        ids::GOLDEN_APPLE_ENCHANTED => food(4, 1.2, false),
        ids::MUSHROOM_STEW => food(6, 0.6, false),
        ids::BREAD => food(5, 0.6, false),
        ids::RAW_PORKCHOP => food(3, 0.3, true),
        ids::COOKED_PORKCHOP => food(8, 0.8, true),
        ids::RAW_FISH => food(2, 0.1, true),
        ids::RAW_SALMON => food(2, 0.1, true),
        ids::CLOWNFISH => food(1, 0.1, true),
        ids::PUFFERFISH => food(1, 0.1, true),
        ids::COOKED_FISH => food(5, 0.6, true),
        ids::COOKED_SALMON => food(6, 0.8, true),
        ids::COOKIE => food(2, 0.1, false),
        ids::MELON => food(2, 0.3, false),
        ids::RAW_BEEF => food(3, 0.3, true),
        ids::STEAK => food(8, 0.8, true),
        ids::RAW_CHICKEN => food(2, 0.3, true),
        ids::COOKED_CHICKEN => food(6, 0.6, true),
        ids::ROTTEN_FLESH => food(4, 0.1, true),
        ids::SPIDER_EYE => food(2, 0.8, false),
        ids::CARROT => food(3, 0.6, false),
        ids::POTATO => food(1, 0.3, false),
        ids::BAKED_POTATO => food(5, 0.6, false),
        ids::POISONOUS_POTATO => food(2, 0.3, false),
        ids::GOLDEN_CARROT => food(6, 1.2, false),
        ids::PUMPKIN_PIE => food(8, 0.3, false),
        ids::RAW_RABBIT => food(3, 0.3, true),
        ids::COOKED_RABBIT => food(5, 0.6, true),
        ids::RABBIT_STEW => food(10, 0.6, true),
        ids::BEETROOT => food(1, 0.6, false),
        ids::BEETROOT_SOUP => food(6, 0.6, false),
        _ => return None,
    };
    Some(f)
}

pub fn is_food(id: i16) -> bool {
    food_of(id).is_some()
}

// 原版 Player::eat(nutrition, satModifier) 的纯粹计算:
//   hunger += nutrition (上限 20)
//   saturation = min(saturation + 2*mod*nutrition, hunger)
// 返回 (新饥饿, 新饱和)
pub fn apply_eat(hunger: f32, saturation: f32, f: Food) -> (f32, f32) {
    let new_hunger = (hunger + f.nutrition as f32).min(MAX_FOOD);
    let saturation_gain = 2.0 * f.saturation * f.nutrition as f32;
    let new_saturation = (saturation + saturation_gain).min(new_hunger).max(0.0);
    (new_hunger, new_saturation)
}

// 原版 HungerAttributeDelegate::notify: 疲劳达到 4 时扣饥饿/饱和
// 返回 (新饥饿, 新饱和, 新疲劳)
pub fn apply_exhaustion(hunger: f32, saturation: f32, exhaustion: f32, amount: f32) -> (f32, f32, f32) {
    let mut exhaustion = exhaustion + amount;
    let mut hunger = hunger;
    let mut saturation = saturation;
    while exhaustion >= MAX_EXHAUSTION {
        exhaustion -= MAX_EXHAUSTION;
        if saturation > 0.0 {
            saturation = (saturation - 1.0).max(0.0);
        } else if hunger > 0.0 {
            hunger = (hunger - 1.0).max(0.0);
        }
    }
    (hunger, saturation, exhaustion)
}

// 原版 FoodConstants: 饥饿<=STARVE_LEVEL 掉血, >=HEAL_LEVEL 且受伤回血
pub const HEAL_LEVEL: f32 = 18.0;
pub const STARVE_LEVEL: f32 = 0.0;
// 每 80 tick(4 秒) 触发一次回血/饿死 与 ITX (currentTick % 80)==0 一致
pub const HEALTH_TICK_COUNT: u32 = 80;
// 回血附加疲劳 原版 EXHAUSTION_HEAL=3
pub const EXHAUSTION_HEAL: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_values_match_original() {
        let apple = food_of(ids::APPLE).unwrap();
        assert_eq!(apple.nutrition, 4);
        assert!((apple.saturation - 0.3).abs() < 1e-6);
        assert!(!apple.meat);
        let steak = food_of(ids::STEAK).unwrap();
        assert_eq!(steak.nutrition, 8);
        assert!((steak.saturation - 0.8).abs() < 1e-6);
        assert!(steak.meat);
        let golden = food_of(ids::GOLDEN_APPLE).unwrap();
        assert!((golden.saturation - 1.2).abs() < 1e-6);
        assert!(food_of(ids::STONE).is_none());
    }

    #[test]
    fn apply_eat_matches_original() {
        // 原版: hunger += nutrition, saturation += 2*mod*nutrition 但不超过 hunger
        let apple = food_of(ids::APPLE).unwrap(); // 4 营养 0.3 饱和
        let (h, s) = apply_eat(10.0, 2.0, apple);
        assert_eq!(h, 14.0);
        // 2 * 0.3 * 4 = 2.4 -> 2 + 2.4 = 4.4
        assert!((s - 4.4).abs() < 1e-5);
        // 满饥饿时饱和不超过饥饿
        let (h, s) = apply_eat(20.0, 0.0, apple);
        assert_eq!(h, 20.0);
        assert!(s <= h);
    }

    #[test]
    fn exhaustion_drains_saturation_then_hunger() {
        let (h, s, e) = apply_exhaustion(20.0, 5.0, 0.0, 4.0);
        assert_eq!(h, 20.0);
        assert!((s - 4.0).abs() < 1e-5); // 先扣饱和
        assert!(e < 4.0);
        // 饱和为 0 时扣饥饿
        let (h, s, e) = apply_exhaustion(20.0, 0.0, 0.0, 4.0);
        assert_eq!(h, 19.0, "饱和为 0 时应扣一格饥饿");
        assert_eq!(s, 0.0);
        assert_eq!(e, 0.0);
    }

    #[test]
    fn all_original_foods_present() {
        // items.json 里 32 种食物 本表全部覆盖
        let ids_food = [
            ids::APPLE, ids::GOLDEN_APPLE, ids::GOLDEN_APPLE_ENCHANTED, ids::MUSHROOM_STEW,
            ids::BREAD, ids::RAW_PORKCHOP, ids::COOKED_PORKCHOP, ids::RAW_FISH, ids::RAW_SALMON,
            ids::CLOWNFISH, ids::PUFFERFISH, ids::COOKED_FISH, ids::COOKED_SALMON, ids::COOKIE,
            ids::MELON, ids::RAW_BEEF, ids::STEAK, ids::RAW_CHICKEN, ids::COOKED_CHICKEN,
            ids::ROTTEN_FLESH, ids::SPIDER_EYE, ids::CARROT, ids::POTATO, ids::BAKED_POTATO,
            ids::POISONOUS_POTATO, ids::GOLDEN_CARROT, ids::PUMPKIN_PIE, ids::RAW_RABBIT,
            ids::COOKED_RABBIT, ids::RABBIT_STEW, ids::BEETROOT, ids::BEETROOT_SOUP,
        ];
        assert_eq!(ids_food.len(), 32);
        for id in ids_food {
            assert!(is_food(id), "食物 {id} 缺失");
        }
    }
}
