//! 对应 `rtg/population/`：RTG 的装饰阶段基础设施。
//!
//! PHP 那边 `BlockWorld` 是一组接口（`BufferedWorld` / `NativePopulationWorld` /
//! `LightWorld` …），全都围绕 `ChunkPrimer` 的 256 高空间。Rust 这边区块是
//! 128 高的 `Chunk`，由 `GenWorld` 做跨区块视图，所以不再抽 trait，
//! 方块读取直接走 `GenWorld::block_state`（state = id << 4 | meta）。
//!
//! `Pos` 对应 PHP 的 `BlockPos`：装饰物大量用 `down()/up()/north()` 这类相对坐标。

pub mod block_rules;

/// 对应 `rtg/population/BlockPos.php`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn add(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.z + dz)
    }

    /// 对应 PHP `BlockPos::__construct` 里的 `(int)floor($x)`。
    ///
    /// RTG 的树枝落点会把 `0.5 + j * step` 这种浮点偏移直接传进 `add()`，
    /// 构造函数再 floor。注意是 floor 不是截断：负数会往更小的一格落。
    pub fn add_f64(self, dx: f64, dy: f64, dz: f64) -> Self {
        Self::new(
            (self.x as f64 + dx).floor() as i32,
            (self.y as f64 + dy).floor() as i32,
            (self.z as f64 + dz).floor() as i32,
        )
    }

    pub fn up(self) -> Self {
        self.add(0, 1, 0)
    }

    pub fn up_n(self, n: i32) -> Self {
        self.add(0, n, 0)
    }

    pub fn down(self) -> Self {
        self.add(0, -1, 0)
    }

    pub fn down_n(self, n: i32) -> Self {
        self.add(0, -n, 0)
    }

    pub fn north(self) -> Self {
        self.add(0, 0, -1)
    }

    pub fn south(self) -> Self {
        self.add(0, 0, 1)
    }

    pub fn west(self) -> Self {
        self.add(-1, 0, 0)
    }

    pub fn east(self) -> Self {
        self.add(1, 0, 0)
    }
}
