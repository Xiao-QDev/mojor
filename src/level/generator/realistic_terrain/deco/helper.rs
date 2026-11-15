//! 对应 `rtg/deco/DecoHelperThisOrThat.php`。
//!
//! 按比例在两个装饰器之间二选一。RTG 用它做"这片长橡木、那片长桦木"的切换：
//! 平原配置是 `(4, 1, oak, birch)`，即 `nextInt(4) != 0` 时用橡木，否则用桦木，
//! 实际是 3:1。

use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::world::GenWorld;

pub struct DecoHelperThisOrThat {
    pub chance: i32,
    /// 0 = `nextInt(chance) == 0` 时选 `this`；1 = `!= 0` 时选 `this`
    pub chance_type: i32,
    pub this: Box<dyn Deco>,
    pub that: Box<dyn Deco>,
}

impl DecoHelperThisOrThat {
    pub fn new(chance: i32, chance_type: i32, this: Box<dyn Deco>, that: Box<dyn Deco>) -> Self {
        // 对应 PHP 构造里的 properlyDefined 检查
        assert!(
            this.properly_defined(),
            "RTG terrain invariant: this 装饰配置残缺"
        );
        assert!(
            that.properly_defined(),
            "RTG terrain invariant: that 装饰配置残缺"
        );
        Self {
            chance,
            chance_type,
            this,
            that,
        }
    }
}

impl Deco for DecoHelperThisOrThat {
    fn generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        tree_density_multiplier: f64,
        has_village: bool,
        chunk_info: &mut ChunkInfo,
    ) {
        // 先掷骰子，再决定跑哪个分支——RNG 消耗与分支无关
        let pick_this = match self.chance_type {
            0 => rand.next_int_bound(self.chance) == 0,
            1 => rand.next_int_bound(self.chance) != 0,
            _ => return,
        };
        let selected = if pick_this {
            &mut self.this
        } else {
            &mut self.that
        };
        selected.generate(
            rtg_world,
            world,
            rand,
            chunk_x,
            chunk_z,
            tree_density_multiplier,
            has_village,
            chunk_info,
        );
    }

    fn properly_defined(&self) -> bool {
        self.this.properly_defined() && self.that.properly_defined()
    }
}

/// 对应 `rtg/deco/DecoHelperRandomSplit.php`：按权重从多个装饰器中选一个执行。
///
/// 巨型针叶林用它在三种树（云杉/小云杉/黄松）之间按 8:2:2 分配。
pub struct DecoHelperRandomSplit {
    pub decos: Vec<Box<dyn Deco>>,
    pub chances: Vec<i32>,
}

impl DecoHelperRandomSplit {
    pub fn new(decos: Vec<Box<dyn Deco>>, chances: Vec<i32>) -> Self {
        assert_eq!(
            decos.len(),
            chances.len(),
            "RTG terrain invariant: decos 和 chances 长度必须一致"
        );
        assert!(!decos.is_empty(), "RTG terrain invariant: decos 不能为空");
        Self { decos, chances }
    }
}

impl Deco for DecoHelperRandomSplit {
    fn generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        tree_density_multiplier: f64,
        has_village: bool,
        chunk_info: &mut ChunkInfo,
    ) {
        let total: i32 = self.chances.iter().sum();
        let mut chosen = rand.next_int_bound(total);
        for (deco, &chance) in self.decos.iter_mut().zip(self.chances.iter()) {
            if chosen < chance {
                deco.generate(
                    rtg_world,
                    world,
                    rand,
                    chunk_x,
                    chunk_z,
                    tree_density_multiplier,
                    has_village,
                    chunk_info,
                );
                return;
            }
            chosen -= chance;
        }
    }

    fn properly_defined(&self) -> bool {
        self.decos.iter().all(|d| d.properly_defined())
    }
}

/// 对应 `rtg/deco/DecoHelper5050.php`：50% 概率跑 `deco1`，否则跑 `deco2`。
///
/// 极端丘陵用它在橡木色和云杉色的黑松/倒木/灌木之间二选一。
pub struct DecoHelper5050 {
    pub deco1: Box<dyn Deco>,
    pub deco2: Box<dyn Deco>,
}

impl DecoHelper5050 {
    pub fn new(deco1: Box<dyn Deco>, deco2: Box<dyn Deco>) -> Self {
        assert!(deco1.properly_defined(), "RTG terrain invariant: deco1 残缺");
        assert!(deco2.properly_defined(), "RTG terrain invariant: deco2 残缺");
        Self { deco1, deco2 }
    }
}

impl Deco for DecoHelper5050 {
    fn generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        tree_density_multiplier: f64,
        has_village: bool,
        chunk_info: &mut ChunkInfo,
    ) {
        if rand.next_boolean() {
            self.deco1.generate(
                rtg_world,
                world,
                rand,
                chunk_x,
                chunk_z,
                tree_density_multiplier,
                has_village,
                chunk_info,
            );
        } else {
            self.deco2.generate(
                rtg_world,
                world,
                rand,
                chunk_x,
                chunk_z,
                tree_density_multiplier,
                has_village,
                chunk_info,
            );
        }
    }

    fn properly_defined(&self) -> bool {
        self.deco1.properly_defined() && self.deco2.properly_defined()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco};
    use crate::level::generator::realistic_terrain::math::JavaRandom;
    use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
    use crate::level::generator::world::GenWorld;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MarkerDeco {
        calls: &'static AtomicUsize,
    }

    impl Deco for MarkerDeco {
        fn generate(
            &mut self,
            _rtg_world: &RtgWorld,
            _world: &mut GenWorld,
            _rand: &mut JavaRandom,
            _chunk_x: i32,
            _chunk_z: i32,
            _tree_density_multiplier: f64,
            _has_village: bool,
            _chunk_info: &mut ChunkInfo,
        ) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn random_split_picks_one_by_weight() {
        let a: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
        let b: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
        let mut split = DecoHelperRandomSplit::new(
            vec![Box::new(MarkerDeco { calls: a }), Box::new(MarkerDeco { calls: b })],
            vec![1, 1],
        );
        let mut rand = JavaRandom::new(42);
        // 跑多次确认每次只触发一个 deco
        for _ in 0..20 {
            let chunks: &'static mut std::collections::HashMap<(i32, i32), crate::level::format::chunk::Chunk> =
                Box::leak(Box::new(std::collections::HashMap::new()));
            chunks.insert((0, 0), crate::level::format::chunk::Chunk::empty(0, 0));
            let mut world = GenWorld::new(chunks);
            let rtg_world = RtgWorld::new(0);
            let mut info = ChunkInfo::new(0, 0);
            split.generate(&rtg_world, &mut world, &mut rand, 0, 0, 1.0, false, &mut info);
        }
        let total = a.load(Ordering::SeqCst) + b.load(Ordering::SeqCst);
        assert_eq!(total, 20, "每次 generate 应只触发一个 deco");
        assert!(a.load(Ordering::SeqCst) > 0 && b.load(Ordering::SeqCst) > 0, "两个分支都应被选中过");
    }

    #[test]
    fn random_split_properly_defined_requires_all_children() {
        let a: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
        let b: &'static AtomicUsize = Box::leak(Box::new(AtomicUsize::new(0)));
        let split = DecoHelperRandomSplit::new(
            vec![Box::new(MarkerDeco { calls: a }), Box::new(MarkerDeco { calls: b })],
            vec![1, 1],
        );
        assert!(split.properly_defined());
    }
}
