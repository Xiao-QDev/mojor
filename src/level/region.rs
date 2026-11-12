// 世界按 region 切分 Folia 的模型是一个 region 交给一个独立的 tick 单元
// 这里一个 region 正好等于一个 anvil 文件的范围 32x32 区块 512x512 方块
// 好处是存档文件与内存分片一一对应 卸载 region 就等于关掉一个 mca
pub const REGION_SHIFT: i32 = 5;
pub const REGION_CHUNKS: i32 = 1 << REGION_SHIFT;
pub const REGION_BLOCKS: i32 = REGION_CHUNKS * 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionId {
    pub x: i32,
    pub z: i32,
}

impl RegionId {
    pub fn of_chunk(chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            x: chunk_x >> REGION_SHIFT,
            z: chunk_z >> REGION_SHIFT,
        }
    }

    pub fn of_block(x: i32, z: i32) -> Self {
        Self::of_chunk(x >> 4, z >> 4)
    }

    // region 覆盖的区块范围 闭区间
    pub fn chunk_bounds(self) -> ((i32, i32), (i32, i32)) {
        (
            (self.x << REGION_SHIFT, self.z << REGION_SHIFT),
            (
                ((self.x + 1) << REGION_SHIFT) - 1,
                ((self.z + 1) << REGION_SHIFT) - 1,
            ),
        )
    }

    pub fn contains_chunk(self, chunk_x: i32, chunk_z: i32) -> bool {
        let ((min_x, min_z), (max_x, max_z)) = self.chunk_bounds();
        (min_x..=max_x).contains(&chunk_x) && (min_z..=max_z).contains(&chunk_z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_matches_anvil_file_split() {
        // anvil 是 32 个区块一个文件 region 也要按同样的边界切
        assert_eq!(RegionId::of_chunk(0, 0), RegionId { x: 0, z: 0 });
        assert_eq!(RegionId::of_chunk(31, 31), RegionId { x: 0, z: 0 });
        assert_eq!(RegionId::of_chunk(32, 0), RegionId { x: 1, z: 0 });
        assert_eq!(RegionId::of_chunk(-1, -1), RegionId { x: -1, z: -1 });
        assert_eq!(RegionId::of_chunk(-32, -1), RegionId { x: -1, z: -1 });
        assert_eq!(RegionId::of_chunk(-33, 0), RegionId { x: -2, z: 0 });
    }

    #[test]
    fn bounds_cover_whole_region() {
        let id = RegionId { x: 0, z: 0 };
        let ((min_x, min_z), (max_x, max_z)) = id.chunk_bounds();
        assert_eq!((min_x, min_z, max_x, max_z), (0, 0, 31, 31));
        assert!(id.contains_chunk(31, 31));
        assert!(!id.contains_chunk(32, 0));
    }
}
