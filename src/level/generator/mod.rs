// 地形生成器 目录结构对齐 Nukkit 的 generator 包
pub mod biome;
pub mod blocks;
pub mod flat;
pub mod noise;
pub mod populator;
pub mod random;
pub mod terrain;
pub mod world;

use std::collections::HashMap;

use crate::level::format::chunk::Chunk;
use terrain::TerrainGenerator;
use world::GenWorld;

// 世界生成方式 平坦用于测试 噪声地形是默认玩法
#[derive(Default, Clone)]
pub enum Generator {
    #[default]
    Flat,
    // 噪声生成器带多张置换表 装箱避免枚举过大
    Terrain(Box<TerrainGenerator>),
}

impl Generator {
    pub fn flat() -> Self {
        Self::Flat
    }

    pub fn terrain(seed: u32) -> Self {
        Self::Terrain(Box::new(TerrainGenerator::new(seed)))
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
        }
    }

    // 第二阶段：洞穴 矿石 树木 植被 只有噪声地形需要
    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        if let Self::Terrain(generator) = self {
            generator.populate_chunk(world, chunk_x, chunk_z);
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
