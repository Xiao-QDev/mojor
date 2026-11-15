// 世界生成目录分两层：
//   共享基础设施 —— blocks / biome / noise / random / world / spline
//   具体生成器   —— vanilla（MCPE 0.14 原版算法）、flat（测试用）、
//                   realistic_terrain（RTG 移植，默认玩法）
// 新增生成器时在 generator/ 下建目录，再给 Generator 加一个变体。
pub mod biome;
pub mod blocks;
pub mod flat;
pub mod noise;
pub mod population;
pub mod random;
pub mod realistic_terrain;
pub mod vanilla;
pub mod world;

// JE 1.18 的三次样条，给新生成器准备的脚手架，暂时还没有调用方。
// 不删除：接新生成器时直接复用，删了还得重写一遍。
#[allow(dead_code)]
mod spline;

// 兼容旧的扁平路径：generator::terrain / genlayer / mcpe_noise / populator
// 外部调用点（level.rs、chunk_manager.rs、network/tests.rs 等）不用改。
pub use vanilla::{genlayer, mcpe_noise, populator, terrain};

use std::collections::HashMap;

use crate::level::format::chunk::Chunk;
use realistic_terrain::terrain_generator::TerrainGenerator as RealisticTerrain;
use vanilla::terrain::TerrainGenerator;
use world::GenWorld;

// 世界生成方式 平坦用于测试 realistic 是默认玩法 terrain 是旧的 MCPE 噪声地形
#[derive(Default, Clone)]
pub enum Generator {
    #[default]
    Flat,
    // 噪声生成器带多张置换表 装箱避免枚举过大
    Terrain(Box<TerrainGenerator>),
    // RTG 移植版 高度图/地表/carver 都是逐位对齐 PHP 的 装箱同理
    Realistic(Box<RealisticTerrain>),
}

impl Generator {
    pub fn flat() -> Self {
        Self::Flat
    }

    pub fn terrain(seed: u32) -> Self {
        Self::Terrain(Box::new(TerrainGenerator::new(seed)))
    }

    pub fn realistic(seed: u32) -> Self {
        Self::Realistic(Box::new(RealisticTerrain::new(seed as i64)))
    }

    // 启动日志用
    pub fn name(&self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Terrain(_) => "terrain",
            Self::Realistic(_) => "rtg",
        }
    }

    // 第一阶段：铺基础地形 只写方块不做装饰
    pub fn generate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        match self {
            Self::Flat => {
                let chunk = flat::generate(chunk_x, chunk_z);
                if let Some(slot) = world.chunk_mut(chunk_x, chunk_z) {
                    *slot = chunk;
                }
            }
            Self::Terrain(generator) => generator.generate_chunk(world, chunk_x, chunk_z),
            Self::Realistic(generator) => {
                let (primer, biomes) = generator.generate_fitted(chunk_x, chunk_z);
                let chunk =
                    realistic_terrain::adapter::to_chunk(&primer, &biomes, chunk_x, chunk_z);
                if let Some(slot) = world.chunk_mut(chunk_x, chunk_z) {
                    *slot = chunk;
                }
            }
        }
    }

    // 第二阶段：洞穴 矿石 树木 植被
    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        match self {
            Self::Terrain(generator) => generator.populate_chunk(world, chunk_x, chunk_z),
            // RTG 的人口阶段：村庄方块结构、树木和 Forest 植被已接，其余阶段还没搬
            Self::Realistic(generator) => generator.populate_chunk(world, chunk_x, chunk_z),
            Self::Flat => {}
        }
    }

    // 生成一个独立的区块 不含装饰 主要给测试用
    pub fn generate(&mut self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut chunks = HashMap::new();
        chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
        {
            let mut world = GenWorld::new(&mut chunks);
            self.generate_chunk(&mut world, chunk_x, chunk_z);
        }
        chunks.remove(&(chunk_x, chunk_z)).expect("刚生成的区块")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RTG 的 state = id << 4 | meta，基岩是 7
    const BEDROCK: u8 = 7;
    // 地表常见方块：草/土/沙/沙砾/沙岩/雪层
    const GROUND: [u8; 7] = [1, 2, 3, 12, 13, 24, 80];

    #[test]
    fn realistic_chunk_has_terrain() {
        let mut generator = Generator::realistic(1337);
        let chunk = generator.generate(0, 0);

        assert_eq!(chunk.block(0, 0, 0), BEDROCK);

        let mut covered = 0;
        for x in 0..16usize {
            for z in 0..16usize {
                let height = chunk.height(x, z) as i32;
                assert!(
                    (1..128).contains(&height),
                    "({x},{z}) 高度 {height} 超出 128 高世界"
                );
                assert_ne!(chunk.biome_id(x, z), 0, "({x},{z}) biome 未设置");
                if (0..128).any(|y| GROUND.contains(&chunk.block(x, y, z))) {
                    covered += 1;
                }
            }
        }
        assert!(covered > 200, "只有 {covered} 列有地表方块");
    }

    #[test]
    fn realistic_generator_is_cloneable_per_region() {
        let mut template = Generator::realistic(2024);
        let mut copy = template.clone();
        assert_eq!(
            template.generate(3, -5).serialize(),
            copy.generate(3, -5).serialize(),
            "每个 region 的生成器副本必须给出同样的地形"
        );
    }
}
