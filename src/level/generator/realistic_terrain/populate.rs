//! 对应 `rtg/population/stage/PopulationStage.php` 的装饰阶段。
//!
//! 参考实现的完整顺序是：
//!
//! ```text
//! 地形 → 结构（村庄/矿道/据点/神殿）→ 湖泊 → 地牢 → rDecorate
//! ```
//!
//! 目前接了村庄方块结构，以及树木和 Forest 的倒木、灌木、花
//! （`BiomeDecorationStage` → `DecoCollectionBase`）。村民/箱子战利品、湖泊、
//! 其他结构和地牢还没接。
//!
//! 随机源必须和参考实现同源：`JavaRandom(RTGWorld::getChunkSeed(chunkX, chunkZ))`，
//! 而不是随便拿区块坐标哈希——否则树的落点和参考实现完全对不上。

use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, collections, generate_vanilla_decoration,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain_generator::TerrainGenerator;
use crate::level::generator::world::GenWorld;

/// 对应 `BiomeDecorationStage::decorate` 的树部分。
pub fn populate_chunk(
    generator: &mut TerrainGenerator,
    world: &mut GenWorld,
    chunk_x: i32,
    chunk_z: i32,
) {
    let seed = generator.chunk_seed(chunk_x, chunk_z);
    let mut rand = JavaRandom::new(seed);
    // PopulationStage：结构先于 biome 装饰，并共享同一条人口 RNG。
    let has_village = generator.populate_villages(world, &mut rand, chunk_x, chunk_z);

    let biome = decoration_biome(world, chunk_x, chunk_z);
    let mut decos = collections::decos_for_biome(biome);

    if !decos.is_empty() {
        let mut chunk_info = ChunkInfo::new(chunk_x, chunk_z);
        let rtg_world = generator.rtg_world();
        for deco in decos.iter_mut() {
            // treeDensityMultiplier 来自服务器配置，默认 1.0
            deco.generate(
                rtg_world,
                world,
                &mut rand,
                chunk_x,
                chunk_z,
                1.0,
                has_village,
                &mut chunk_info,
            );
        }
    }

    // VanillaBiomeDecorationStage：RTG deco 之后接原版 BiomeDecorator
    // （矿石 → 斑块 → 树 → 花 → 草 → ... → 各 biome 钩子）。
    generate_vanilla_decoration(world, &mut rand, chunk_x, chunk_z, biome);
}

/// 对应 `BiomeDecorationStage::resolve`：取区块右下角 `+16,+16` 那一格作为装饰 biome。
///
/// PHP 侧 `getBiome` 是纯算的，不需要区块加载；我们这边从已生成的区块读 biome 表，
/// 如果那个区块还没就位就退回本区块中心——宁可少刷几棵树，也不能读到 ocean 默认值。
fn decoration_biome(world: &GenWorld, chunk_x: i32, chunk_z: i32) -> u8 {
    let sample_x = chunk_x * 16 + 16;
    let sample_z = chunk_z * 16 + 16;
    if world.chunk(sample_x >> 4, sample_z >> 4).is_some() {
        return world.biome(sample_x, sample_z);
    }
    world.biome(chunk_x * 16 + 8, chunk_z * 16 + 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn empty_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -1..=1 {
            for chunk_z in -1..=1 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        GenWorld::new(chunks)
    }

    /// 平原的树很稀疏（`noise > 0.4` 再乘 1/48），所以铺一大片平地、
    /// 跑 400 个区块来验证它确实会种。
    #[test]
    #[ignore]
    fn plains_grows_trees_over_many_chunks() {
        let generator = TerrainGenerator::new(20241115);
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        const SIZE: i32 = 20;
        for chunk_x in 0..SIZE {
            for chunk_z in 0..SIZE {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        // 草面在 y=63，和 `tree::betula` 的测试床一致
        for x in 0..SIZE * 16 {
            for z in 0..SIZE * 16 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 32 } else { 48 });
                }
            }
        }

        let mut rand = JavaRandom::new(1);
        let mut info = ChunkInfo::new(0, 0);
        let mut decos = collections::plains_decos();
        for chunk_x in 0..SIZE {
            for chunk_z in 0..SIZE {
                for deco in decos.iter_mut() {
                    deco.generate(
                        generator.rtg_world(),
                        &mut world,
                        &mut rand,
                        chunk_x,
                        chunk_z,
                        1.0,
                        false,
                        &mut info,
                    );
                }
            }
        }

        let mut logs = 0;
        let mut leaves = 0;
        for x in 0..SIZE * 16 {
            for z in 0..SIZE * 16 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        17 => logs += 1,
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        println!("{} 个区块里：原木 {logs} 个，树叶 {leaves} 个", SIZE * SIZE);
        assert!(logs > 0, "应该长出至少一棵树");
    }

    #[test]
    fn uses_neighbour_chunk_when_loaded() {
        let mut world = empty_world();
        world.set_biome(8, 8, 1, 0); // 本区块中心：平原
        world.set_biome(16, 16, 4, 0); // 右下角：森林
        assert_eq!(decoration_biome(&world, 0, 0), 4);
    }

    #[test]
    fn falls_back_to_chunk_center_when_neighbour_missing() {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_biome(8, 8, 1, 0);
        chunks.insert((0, 0), chunk);
        let world = GenWorld::new(chunks);
        // 区块 (1,1) 没加载，退回本区块中心（平原）
        assert_eq!(decoration_biome(&world, 0, 0), 1);
    }
}
