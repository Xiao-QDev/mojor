//! 对应 `rtg/tree/TreeDensityLimiter.php`。
//!
//! RTG 的"树密度预算"：一个区块里有 `loopCount` 棵树的名额，每长一棵就按它的
//! 体积（`estimatedSize()`）扣掉一点。名额不够时按剩余比例掷骰子，一旦失败就把
//! 名额清零——所以密林里偶尔会出现"这片就长到这儿"的效果。

use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::f;

pub struct TreeDensityLimiter {
    trees_remaining: f64,
    pub trees_so_far: i32,
}

impl TreeDensityLimiter {
    pub fn new(tree_limit: i32) -> Self {
        Self {
            trees_remaining: f(tree_limit as f64),
            trees_so_far: 0,
        }
    }

    /// 想再长一棵体积为 `needed` 的树，够不够名额。
    pub fn allowed(&mut self, needed: f64, rand: &mut JavaRandom) -> bool {
        let needed = f(needed);
        if self.trees_remaining <= 0.0 {
            return false;
        }
        if self.trees_remaining >= needed {
            self.trees_so_far += 1;
            self.trees_remaining = f(self.trees_remaining - needed);
            return true;
        }
        // 名额不够：按剩余比例掷一次，无论结果都把名额用完
        let allowed = f((rand.next_float() as f64) * needed) < self.trees_remaining;
        if allowed {
            self.trees_so_far += 1;
        }
        self.trees_remaining = 0.0;
        allowed
    }

    pub fn not_done(&self) -> bool {
        self.trees_remaining > 0.0
    }

    /// 占掉名额但没真正长树（比如地形太高）。
    pub fn occupy(&mut self, occupied: f64) {
        self.trees_so_far += 1;
        self.trees_remaining = f(self.trees_remaining - f(occupied));
    }

    pub fn remaining(&self) -> f64 {
        f(self.trees_remaining)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_until_budget_runs_out() {
        let mut limiter = TreeDensityLimiter::new(3);
        let mut rand = JavaRandom::new(1);
        // 每棵体积 1.0，名额 3，应该正好三棵
        assert!(limiter.allowed(1.0, &mut rand));
        assert!(limiter.allowed(1.0, &mut rand));
        assert!(limiter.allowed(1.0, &mut rand));
        assert!(!limiter.not_done());
        assert!(!limiter.allowed(1.0, &mut rand));
        assert_eq!(limiter.trees_so_far, 3);
    }

    #[test]
    fn overspending_rolls_and_clears_budget() {
        let mut limiter = TreeDensityLimiter::new(2);
        let mut rand = JavaRandom::new(7);
        // 体积 5 超过名额 2：掷骰子，之后名额必为 0
        let _ = limiter.allowed(5.0, &mut rand);
        assert_eq!(limiter.remaining(), 0.0);
        assert!(!limiter.not_done());
    }

    #[test]
    fn occupy_reduces_remaining() {
        let mut limiter = TreeDensityLimiter::new(5);
        limiter.occupy(2.0);
        assert_eq!(limiter.remaining(), 3.0);
        assert_eq!(limiter.trees_so_far, 1);
    }
}
