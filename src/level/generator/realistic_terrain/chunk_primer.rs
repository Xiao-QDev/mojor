//! 对应 `rtg/ChunkPrimer.php`。
//!
//! 每个方块用 RTG 自己的 `state = id << 4 | meta` 记法（例如石头 16、水 144、
//! 雪层 1248）。真实方块映射在 `population::BlockRules`，本层不碰。
//!
//! PHP 用 256 个 256 字节字符串存列；这里换成 `[[u8; 256]; 256]`，
//! 语义完全一致（下标 `[x * 16 + z][y]`，越界 y 直接忽略）。

use crate::level::generator::population::block_rules;

pub const HEIGHT: usize = 256;

pub struct ChunkPrimer {
    ids: Vec<[u8; HEIGHT]>,
    meta: Vec<[u8; HEIGHT]>,
}

impl ChunkPrimer {
    /// `new ChunkPrimer($heights)`：高度以下填石头，海平面 63 以下补水。
    ///
    /// 传进来的是原始噪声高度（f64），PHP 用 `(int)` 截断后再 +1。
    pub fn new(heights: &[f64]) -> Self {
        let mut ids = vec![[0u8; HEIGHT]; 256];
        let meta = vec![[0u8; HEIGHT]; 256];

        for i in 0..256 {
            let stone = ((heights[i] as i32) + 1).clamp(0, HEIGHT as i32) as usize;
            let water = (63 - stone as i32).max(0) as usize;
            let column = &mut ids[i];
            column[..stone].fill(1);
            column[stone..stone + water].fill(9);
        }

        Self { ids, meta }
    }

    #[inline]
    pub fn get_block_state(&self, x: i32, y: i32, z: i32) -> i32 {
        if !(0..=255).contains(&y) {
            return 0;
        }
        let i = (x * 16 + z) as usize;
        ((self.ids[i][y as usize] as i32) << 4) | self.meta[i][y as usize] as i32
    }

    #[inline]
    pub fn set_block_state(&mut self, x: i32, y: i32, z: i32, state: i32) {
        if !(0..=255).contains(&y) {
            return;
        }
        let i = (x * 16 + z) as usize;
        self.ids[i][y as usize] = (state >> 4) as u8;
        self.meta[i][y as usize] = (state & 15) as u8;
    }

    /// 最高非空方块的 y；整列为空时返回 0。
    pub fn find_ground_block_idx(&self, x: i32, z: i32) -> i32 {
        let column = &self.ids[(x * 16 + z) as usize];
        let mut top = HEIGHT;
        while top > 0 && column[top - 1] == 0 {
            top -= 1;
        }
        (top as i32 - 1).max(0)
    }

    pub fn ids(&self) -> &Vec<[u8; HEIGHT]> {
        &self.ids
    }

    pub fn meta(&self) -> &Vec<[u8; HEIGHT]> {
        &self.meta
    }

    /// 32 位 FNV-1a 校验和：与 PHP 侧同样的循环顺序（先 ids 后 meta，逐列），
    /// 用来把整块 256×256 方块与 PHP 参考实现逐字节比对。
    pub fn fingerprint(&self) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        for i in 0..256 {
            for &b in self.ids[i].iter().chain(self.meta[i].iter()) {
                hash ^= b as u32;
                hash = hash.wrapping_mul(0x0100_0193);
            }
        }
        hash
    }

    /// 对应 `ChunkPrimer::terrainHeights()`：每列最高「实心且非树叶」方块的 y+1。
    ///
    /// PHP 返回 512 字节小端串（`chr($h & 255) . chr($h >> 8)`），这里直接给数组。
    /// 树叶不算实心——否则树冠会把高度抬高，村庄 / mansion 的高度探针会偏。
    pub fn terrain_heights(&self) -> [u16; 256] {
        let mut heights = [0u16; 256];
        for (index, column) in self.ids.iter().enumerate() {
            let mut y = HEIGHT as i32 - 1;
            while y >= 0 {
                let id = column[y as usize];
                let material = block_rules::material((id as i32) << 4);
                if block_rules::material_is_solid(material) && material != block_rules::LEAVES {
                    break;
                }
                y -= 1;
            }
            heights[index] = (y + 1) as u16;
        }
        heights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_heights_skips_water() {
        // 高度 10 → 石头填 y=0..10，水从 11 到 62
        let primer = ChunkPrimer::new(&[10.0; 256]);
        assert!(primer.terrain_heights().iter().all(|&height| height == 11));
    }

    #[test]
    fn terrain_heights_ignores_leaves() {
        let mut primer = ChunkPrimer::new(&[10.0; 256]);
        // 在石头上叠树叶，高度不该被抬高
        primer.set_block_state(0, 11, 0, 18 << 4);
        assert_eq!(primer.terrain_heights()[0], 11);
        // 换成木头就算实心
        primer.set_block_state(0, 11, 0, 17 << 4);
        assert_eq!(primer.terrain_heights()[0], 12);
    }
}
