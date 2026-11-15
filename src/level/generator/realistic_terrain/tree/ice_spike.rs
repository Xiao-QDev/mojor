//! 对应 `rtg/tree/IceSpikeRTG.php`：冰刺（冰原 biome 用）。
//!
//! 一根短主干向上延伸 `crownSize` 格，途中随机向 8 方向之一伸出侧枝，
//! 枝干和主干都是冰方块（2784 = packed ice）。`no_leaves=true`，所以不放树叶。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct IceSpikeRTG {
    pub base: TreeRTG,
    /// 对应 `$taper`：侧枝长度随高度递减的斜率。
    pub taper: f64,
}

impl Default for IceSpikeRTG {
    /// 对应 `constructIceSpikeRTG0`。
    fn default() -> Self {
        let mut base = TreeRTG {
            log_block: 2784,
            leaves_block: 0,
            trunk_size: 1,
            crown_size: 13,
            no_leaves: true,
            ..TreeRTG::default()
        };
        // 冰原地面是雪层（block 80, state 1280），也允许。
        base.valid_ground_blocks.push(1280);
        Self {
            base,
            taper: f(0.15000000596046448),
        }
    }
}

impl Tree for IceSpikeRTG {
    fn furthest_likely_extension(&self) -> i32 {
        // crownSize 最大 24，侧枝最长约 24*0.15+0.9 ≈ 4.5，取 5。
        5
    }

    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(ext * ext) / 32.0)
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n2(world, pos) {
            return false;
        }
        let (x, mut y, z) = (pos.x, pos.y, pos.z);
        let trunk_size = self.base.trunk_size;
        let crown_size = self.base.crown_size;
        let log_block = self.base.log_block;

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        for _ in 0..trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y, z));
            y += 1;
        }

        let mut last_direction: i32 = -1;
        let mut branch_wait: i32 = 0;
        for i in 0..crown_size {
            if rand.next_int_bound(4) < 3 && i < crown_size - 2 {
                let length = (f(f((crown_size - i) as f64) * self.taper) + 0.9) as i32;
                if length > 0 && branch_wait <= 0 {
                    let mut direction = rand.next_int_bound(8);
                    if direction == last_direction {
                        direction += 1;
                    }
                    self.build_branches(world, rand, x, y, z, direction, length, &mut tracker);
                    last_direction = direction;
                    branch_wait = 2;
                } else {
                    branch_wait -= 1;
                }
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), log_block);
            if i < crown_size - 2 {
                if rand.next_boolean() {
                    self.build_leaves(world, x, y, z + 1, &mut tracker);
                }
                if rand.next_boolean() {
                    self.build_leaves(world, x, y, z - 1, &mut tracker);
                }
                if rand.next_boolean() {
                    self.build_leaves(world, x + 1, y, z, &mut tracker);
                }
                if rand.next_boolean() {
                    self.build_leaves(world, x - 1, y, z, &mut tracker);
                }
            }
            y += 1;
        }
        self.build_leaves(world, x, y - 1, z + 1, &mut tracker);
        self.build_leaves(world, x, y - 1, z - 1, &mut tracker);
        self.build_leaves(world, x + 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x - 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x, y, z, &mut tracker);
        true
    }
}

impl IceSpikeRTG {
    /// 对应 `dx`：8 方向 → x 偏移。
    fn dx(direction: i32) -> i32 {
        let d = direction % 8;
        if d == 0 || d == 4 {
            0
        } else if d < 5 {
            1
        } else {
            -1
        }
    }

    /// 对应 `dz`：8 方向 → z 偏移（比 dx 多转 2 格）。
    fn dz(direction: i32) -> i32 {
        let d = (direction + 2) % 8;
        if d == 0 || d == 4 {
            0
        } else if d < 5 {
            1
        } else {
            -1
        }
    }

    /// 对应 `buildBranches`：一次发出 3 根侧枝。
    #[allow(clippy::too_many_arguments)]
    fn build_branches(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        direction: i32,
        log_length: i32,
        tracker: &mut SkylightTracker,
    ) {
        let d = direction;
        self.build_branch(world, rand, x, y, z, Self::dx(d), Self::dz(d), log_length, tracker);
        let d2 = direction + 2;
        self.build_branch(world, rand, x, y, z, Self::dx(d2), Self::dz(d2), log_length, tracker);
        let d3 = direction + 5;
        self.build_branch(world, rand, x, y, z, Self::dx(d3), Self::dz(d3), log_length, tracker);
    }

    /// 对应 `buildBranch`：沿 (dX, 1, dZ) 方向放 logLength 根原木。
    #[allow(clippy::too_many_arguments)]
    fn build_branch(
        &self,
        world: &mut GenWorld,
        _rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        dx: i32,
        dz: i32,
        log_length: i32,
        tracker: &mut SkylightTracker,
    ) {
        for m in 1..=log_length {
            self.base.place_log_block(
                world,
                tracker,
                Pos::new(x + dx * m, y + m, z + dz * m),
                self.base.log_block,
            );
        }
    }

    /// 对应 `buildLeaves`：noLeaves 时什么都不做。
    fn build_leaves(
        &self,
        world: &mut GenWorld,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
    ) {
        if !self.base.no_leaves {
            self.base.place_leaves_block(
                world,
                tracker,
                Pos::new(x, y, z),
                self.base.leaves_block,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use std::collections::HashMap;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -2..=2 {
            for chunk_z in -2..=2 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -32..48 {
            for z in -32..48 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 32 } else { 48 });
                }
            }
        }
        world
    }

    #[test]
    fn ice_spike_grows_trunk_and_side_branches() {
        let mut world = flat_world();
        let mut spike = IceSpikeRTG::default();
        spike.base.trunk_size = 1;
        spike.base.crown_size = 13;
        let mut rand = JavaRandom::new(42);
        assert!(spike.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let mut ice = 0;
        for x in -32..48 {
            for z in -32..48 {
                for y in 0..128 {
                    if world.block(x, y, z) == 174 {
                        ice += 1;
                    }
                }
            }
        }
        // 主干 13 根 + 多根侧枝 + 中心顶端
        assert!(ice > 13, "冰刺冰方块太少：{ice}");
    }

    #[test]
    fn ice_spike_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut spike = IceSpikeRTG::default();
        let mut rand = JavaRandom::new(42);
        assert!(!spike.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
