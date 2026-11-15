//! Java `java.util.Random` 的 48 位 LCG。
//!
//! 对应 PHP 源：`example/level/generator/common/math/{JavaLong,JavaRandom}.php`
//!
//! PHP 那边用 22 位分肢手工模拟 64 位溢出（因为 PHP 整型乘法会溢出成 float）。
//! Rust 的 `wrapping_*` 本来就是补码回绕，与 Java 语义一致，所以这里直接算。
//!
//! 唯一必须保留的细节是 `JavaLong::f()` 的 **float32 截断**：
//! RTG 用 `f()` 包住几乎每一步浮点结果，用来模拟 Java 的 `float` 精度。
//! 移植时不保留它，高度场会整体漂移。

const MULTIPLIER: i64 = 0x5_DEEC_E66D;
const ADDEND: i64 = 0xB;
const MASK: i64 = (1_i64 << 48) - 1;

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEED_UNIQUIFIER: AtomicI64 = AtomicI64::new(8_682_522_807_148_012);

/// Java 的 `float` 截断：double 算完立刻按 float32 落地，再升回 double。
///
/// 这是 RTG 全篇最容易被忽略的精度约定，凡是 PHP 里写了 `F::f(...)` 的地方都要包一层。
#[inline]
pub fn f(value: f64) -> f64 {
    value as f32 as f64
}

/// Java `Math.round(float)`：`floor(v + 0.5)`，负数向零取整的方向与 `as i64` 不同。
#[inline]
pub fn round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

// 与 JavaRandom 同源的一对 LCG 常量，被用作 GenLayer 的混合函数。
const MIX_A: i64 = 0x5851_F42D_4C95_7F2D_u64 as i64;
const MIX_B: i64 = 0x1405_7B7E_F767_814F_u64 as i64;

/// `JavaLong::mix(seed, salt)` = `seed * (seed * A + B) + salt`（2^64 回绕）。
///
/// PHP 里用 22 位分肢实现，这里直接乘。GenLayer 的世界种子、区块种子、
/// 每格取随机数都靠它推进，写错则整条 biome 链全废。
#[inline]
pub fn mix(seed: i64, salt: i64) -> i64 {
    seed.wrapping_mul(seed.wrapping_mul(MIX_A).wrapping_add(MIX_B))
        .wrapping_add(salt)
}

/// `java.util.Random`
#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: i64,
}

impl JavaRandom {
    pub fn new(seed: i64) -> Self {
        let mut random = Self { seed: 0 };
        random.set_seed(seed);
        random
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ MULTIPLIER) & MASK;
    }

    /// 对应 `JavaRandom::fromSystem()`：少数装饰故意不使用世界种子。
    pub fn from_system() -> Self {
        let previous = SEED_UNIQUIFIER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |seed| {
                Some(seed.wrapping_mul(181_783_497_276_652_981))
            })
            .expect("原子种子更新闭包总是返回新值");
        let unique = previous.wrapping_mul(181_783_497_276_652_981);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;
        Self::new(unique ^ nanos)
    }

    /// 内部 48 位状态，用于「生成前保存、生成后还原」这类无副作用探测。
    pub fn state(&self) -> i64 {
        self.seed
    }

    pub fn restore(&mut self, state: i64) {
        self.seed = state & MASK;
    }

    /// 推进一格，返回高 `bits` 位（无符号，落在 `0..2^bits`）。
    fn next(&mut self, bits: u32) -> i64 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        self.seed >> (48 - bits)
    }

    pub fn next_int(&mut self) -> i32 {
        self.next(32) as u32 as i32
    }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0 && bound <= i32::MAX, "bound 必须在 1..=i32::MAX");
        // 2 的幂走无偏路径
        if bound & bound.wrapping_neg() == bound {
            return ((bound as i64 * self.next(31)) >> 31) as i32;
        }
        loop {
            let bits = self.next(31) as i32;
            let value = bits % bound;
            // 用 i64 比较：PHP 那边也是 64 位整型，不会溢出
            if (bits as i64) - (value as i64) + (bound as i64 - 1) < 2_147_483_648 {
                return value;
            }
        }
    }

    pub fn next_long(&mut self) -> i64 {
        let hi = self.next_int() as i64;
        let lo = self.next_int() as i64;
        hi.wrapping_shl(32).wrapping_add(lo)
    }

    /// 24 位分子，结果在 float32 里可精确表示，所以直接返回 f32。
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / 16_777_216.0
    }

    pub fn next_double(&mut self) -> f64 {
        let hi = self.next(26) << 27;
        (hi + self.next(27)) as f64 / 9_007_199_254_740_992.0
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 以下期望值由 PHP 原实现导出：php /tmp/rtg_vec.php

    #[test]
    fn int_bound_sequence() {
        let mut r = JavaRandom::new(12345);
        let got: Vec<i32> = (0..6).map(|_| r.next_int_bound(100)).collect();
        assert_eq!(got, vec![51, 80, 41, 28, 55, 84]);
    }

    #[test]
    fn long_float_double_bool() {
        let mut r = JavaRandom::new(12345);
        assert_eq!(r.next_long(), 6_674_089_274_190_705_457);
        assert_eq!(r.next_float(), 0.932_993_472_f32);
        assert_eq!(r.next_double(), 0.917_114_702_360_203_1);
        assert!(!r.next_boolean());
        assert_eq!(r.next_int(), 1_402_202_751);
    }

    #[test]
    fn rtg_chunk_seeds() {
        // RTGWorld::getChunkSeed 用世界种子取两条奇数列步长
        let mut r = JavaRandom::new(1_152_860_609);
        assert_eq!(r.next_long() / 2 * 2 + 1, 1_824_745_349_195_516_115);
        assert_eq!(r.next_long() / 2 * 2 + 1, 669_677_566_737_874_083);
    }

    #[test]
    fn state_round_trip() {
        let mut r = JavaRandom::new(7);
        let saved = r.state();
        let a = r.next_int();
        r.restore(saved);
        assert_eq!(a, r.next_int());
    }

    #[test]
    fn float32_truncation() {
        // 1/3 在 f32 里只有 24 位有效位
        assert_eq!(f(1.0 / 3.0), 0.333_333_34_f32 as f64);
        assert_eq!(round(2.5), 3);
        assert_eq!(round(-2.5), -2);
    }

    #[test]
    fn mix_matches_php() {
        // 期望值来自 JavaLong::mix
        assert_eq!(mix(1, 2), 7_806_831_264_735_756_414);
        assert_eq!(mix(-5, 7), 4_315_977_802_048_595_169);
        assert_eq!(mix(123_456_789, 987_654_321), -4_954_415_829_755_636_303);
    }
}
