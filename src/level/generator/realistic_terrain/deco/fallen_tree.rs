//! 对应 `rtg/deco/DecoVariableFallenTree.php` 和 `rtg/feature/WorldGenLog.php`。
//!
//! 原参考的循环末尾是 `$noise = -.2`，所以每区块最多尝试一根倒木；这里保留这个
//! 看似反常的行为以及放置前的两次地面悬空检查。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, Distribution, get_ranged_random, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

const TREE_MATERIALS_SIMPLEX: usize = 7;
const TREE_MATERIALS_NOISE_DIVISOR: f64 = 631.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallenWoodland {
    Spruce,
    Oak,
    Birch,
}

/// 对应 `DecoVariableFallenTree`。
pub struct DecoVariableFallenTree {
    pub loops: i32,
    pub distribution: Distribution,
    pub max_y: i32,
    pub min_size: i32,
    pub max_size: i32,
    pub woodland: FallenWoodland,
}

impl DecoVariableFallenTree {
    pub fn new(woodland: FallenWoodland) -> Self {
        Self {
            loops: 1,
            distribution: Distribution::new(100.0, 5.0, f(0.8)),
            max_y: 80,
            min_size: 2,
            max_size: 4,
            woodland,
        }
    }

    fn material_for(&self, pos: Pos, rtg_world: &RtgWorld, rand: &mut JavaRandom) -> TreeMaterials {
        let divisor = f(TREE_MATERIALS_NOISE_DIVISOR);
        let x = f(f(pos.x as f64) / divisor);
        let z = f(f(pos.z as f64) / divisor);
        let raw = f(rtg_world
            .simplex_instance(TREE_MATERIALS_SIMPLEX)
            .noise2f(x as f32, z as f32) as f64);
        let value = match self.woodland {
            FallenWoodland::Spruce => f(f(f(
                f(f(raw * f(-1.0)) - f(2.0)) + f(rand.next_float() as f64 * f(2.5))
            ) - f(1.25))),
            FallenWoodland::Oak => f(f(raw + f(rand.next_float() as f64 * f(1.4))) - f(0.7)),
            FallenWoodland::Birch => f(f(f(
                f(f(raw * f(-1.0)) + f(2.0)) + f(rand.next_float() as f64 * f(2.5))
            ) - f(1.25))),
        };
        TreeMaterials::for_noise(value)
    }
}

impl Deco for DecoVariableFallenTree {
    fn generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        _tree_density_multiplier: f64,
        has_village: bool,
        _chunk_info: &mut ChunkInfo,
    ) {
        let offset = offset_pos(chunk_x, chunk_z);
        let mut noise = f(self
            .distribution
            .value(offset, rtg_world.simplex_instance(9)));
        noise = f(f(f(noise / f(2.0)) + f(0.5)));
        noise = f(noise * f(2.0));
        // Forest 的 fallenLogDensityMultiplier 默认为 1.0。
        noise = f(noise * f(1.0));

        let final_size = if self.max_size > self.min_size {
            get_ranged_random(rand, self.min_size, self.max_size)
        } else if self.max_size == self.min_size {
            self.min_size
        } else {
            4
        };

        while noise > 0.0 {
            if (rand.next_float() as f64) < f(noise * f(0.2)) {
                let column = offset.add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
                let pos = column.up_n(surface_height(world, column.x, column.z));
                if pos.y <= self.max_y {
                    if has_village && !block_rules::area_is_air(world, pos, final_size) {
                        return;
                    }
                    let materials = self.material_for(pos, rtg_world, rand);
                    generate_log(world, rand, pos, materials.log, final_size);
                }
            }
            // 参考实现这里是赋值而不是 -=，因此第二次循环必定结束。
            noise = f(-0.2);
        }
    }
}

/// 对应 `rtg/deco/DecoFallenTree.php`：用指定木材的倒木。
///
/// 和 `DecoVariableFallenTree` 不同，这里的木材是写死的（沼泽用金合欢），
/// 生成次数由 `loops` × `log_condition_chance` 决定。`distribution` 字段保留
/// 以对应 PHP 配置，但 PHP 的 generate 里实际没用到它。
pub struct DecoFallenTree {
    pub loops: i32,
    pub distribution: Distribution,
    pub log_condition: i32,
    pub log_condition_chance: i32,
    pub max_y: i32,
    pub log_block: i32,
    pub leaves_block: i32,
    pub min_size: i32,
    pub max_size: i32,
    pub random_log_blocks: Vec<i32>,
}

impl Default for DecoFallenTree {
    /// 对应 `constructDecoFallenTree0`。
    fn default() -> Self {
        Self {
            loops: 1,
            distribution: Distribution::new(100.0, 5.0, f(0.8)),
            log_condition: 1,
            log_condition_chance: 1,
            max_y: 80,
            log_block: 272,
            leaves_block: 296,
            min_size: 2,
            max_size: 4,
            random_log_blocks: Vec::new(),
        }
    }
}

impl DecoFallenTree {
    /// 对应 `isValidLogCondition`。
    fn is_valid_log_condition(&self, rand: &mut JavaRandom) -> bool {
        match self.log_condition {
            0 => true,
            1 => rand.next_int_bound(self.log_condition_chance) == 0,
            _ => false,
        }
    }
}

impl Deco for DecoFallenTree {
    fn generate(
        &mut self,
        _rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        _tree_density_multiplier: f64,
        has_village: bool,
        _chunk_info: &mut ChunkInfo,
    ) {
        // 对应 randomLogBlocks：配了就从中随机选一种木材
        if !self.random_log_blocks.is_empty() {
            let idx = rand.next_int_bound(self.random_log_blocks.len() as i32) as usize;
            self.log_block = self.random_log_blocks[idx];
        }
        // fallenLogDensityMultiplier 这里默认 1.0，chance 不变
        let final_size = if self.max_size > self.min_size {
            get_ranged_random(rand, self.min_size, self.max_size)
        } else if self.max_size == self.min_size {
            self.min_size
        } else {
            4
        };
        for _ in 0..self.loops {
            if self.is_valid_log_condition(rand) {
                let column = offset_pos(chunk_x, chunk_z).add(
                    rand.next_int_bound(16),
                    0,
                    rand.next_int_bound(16),
                );
                let pos = column.up_n(surface_height(world, column.x, column.z));
                if pos.y <= self.max_y {
                    if has_village && !block_rules::area_is_air(world, pos, final_size) {
                        return;
                    }
                    generate_log(world, rand, pos, self.log_block, final_size);
                }
            }
        }
    }
}

/// 对应 `WorldGenLog::generate`；Forest 构造器的 `generateLeaves` 固定为 false。
fn generate_log(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    log_block: i32,
    log_length: i32,
) -> bool {
    let (mut x, y, mut z) = (pos.x, pos.y, pos.z);
    let ground = world.block_state(x, y - 1, z);
    if !matches!(
        block_rules::material(ground),
        block_rules::GROUND | block_rules::GRASS | block_rules::SAND | block_rules::ROCK
    ) {
        return false;
    }

    let dir = rand.next_int_bound(2);
    let (step_x, step_z) = if dir == 0 { (1, 0) } else { (0, 1) };
    let mut air = 0;
    let mut blocks = Vec::new();

    for _ in 0..log_length {
        let state = world.block_state(x - step_x, y, z - step_z);
        if !matches!(
            block_rules::material(state),
            block_rules::AIR | block_rules::VINE | block_rules::PLANTS
        ) {
            break;
        }
        x -= step_x;
        z -= step_z;
        if air_check(world, x, y, z) > 0 {
            return false;
        }
    }

    for _ in 0..log_length * 2 {
        let state = world.block_state(x + step_x, y, z + step_z);
        if !matches!(
            block_rules::material(state),
            block_rules::AIR | block_rules::VINE | block_rules::PLANTS
        ) {
            break;
        }
        air += air_check(world, x, y, z);
        if air > 2 || world.block_state(pos.x, pos.y + 1, pos.z) >> 4 != 0 {
            return false;
        }
        let Some(state) =
            block_rules::with_property(log_block, "log_axis", if dir == 0 { 4 } else { 8 })
        else {
            return false;
        };
        blocks.push((Pos::new(x, y, z), state));
        x += step_x;
        z += step_z;
    }

    for (target, state) in blocks {
        world.set_block_state(target.x, target.y, target.z, state);
    }
    true
}

fn air_check(world: &GenWorld, x: i32, y: i32, z: i32) -> i32 {
    let below = world.block_state(x, y - 1, z);
    if matches!(
        block_rules::material(below),
        block_rules::AIR | block_rules::VINE | block_rules::WATER | block_rules::PLANTS
    ) {
        let below2 = world.block_state(x, y - 2, z);
        if matches!(
            block_rules::material(below2),
            block_rules::AIR | block_rules::VINE | block_rules::WATER | block_rules::PLANTS
        ) {
            return 99;
        }
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    #[test]
    fn log_follows_ground_and_sets_horizontal_axis() {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 32);
                world.set_block_state(x, 62, z, 48);
            }
        }
        let mut rand = JavaRandom::new(7);
        assert!(generate_log(
            &mut world,
            &mut rand,
            Pos::new(8, 64, 8),
            TreeMaterials::OAK.log,
            3,
        ));

        let mut logs = 0;
        for x in 0..16 {
            for z in 0..16 {
                let state = world.block_state(x, 64, z);
                if state >> 4 == 17 {
                    logs += 1;
                    assert!(matches!(state & 12, 4 | 8));
                }
            }
        }
        assert_eq!(logs, 6, "参考第二段循环会放 logLength * 2 根原木");
    }
}
