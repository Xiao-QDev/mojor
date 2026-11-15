//! 对应 `rtg/tree/{SkylightTracker,Spotlight}.php`。
//!
//! RTG 种树前会先问一句"这块地方再加这一块树叶，会不会让森林太暗"。做法是在
//! 树根周围铺一张二维网格，每格记这一列当前的遮光值：
//!
//! - `direct_opacity`：这一列自己的遮光（从世界往下扫 + 每放一块就叠加）
//! - `opacity`：从任意方向传过来的最低遮光（Dijkstra 式扩散，每格 +1）
//! - `trunk`：树干所在列，光线不能穿
//!
//! `test_place` 先把新方块记进去，用 `darken/relight` 重算受影响区域，如果最暗的
//! 一格没有超过 `tolerable_obstruction`（默认 4，深色橡木林 6）才真正落方块。
//! 这就是森林里树不会挤成一坨黑的原因。
//!
//! 二维数组在 PHP 里是 `[$x][$z]`，这里一维化成 `x * width + z`。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::world::GenWorld;

/// 1 = 西(-x)，2 = 北(-z)，3 = 东(+x)，4 = 南(+z)，0 = 无方向。
fn reverse(direction: i32) -> i32 {
    match direction {
        1 => 3,
        2 => 4,
        3 => 1,
        4 => 2,
        _ => 0,
    }
}

/// 对应 `rtg/tree/Spotlight.php`：在网格上游走的游标。
///
/// PHP 里它通过 `$owner` 反向读写信箱；Rust 里把数组留在 `SkylightTracker` 上，
/// 由 tracker 的方法同时接收 `&mut self` 和 `&mut Spotlight`。
#[derive(Clone, Debug, Default)]
pub struct Spotlight {
    pub x: i32,
    pub z: i32,
    /// 沿路累积的遮光，`darken`/`relight` 递归时临时 +1
    pub passed_opacity: i32,
    /// `test_place` 里收集到的最暗值
    pub worst_opacity: i32,
    pub total_back_tracks: i32,
}

impl Spotlight {
    fn at(x: i32, z: i32) -> Self {
        Self {
            x,
            z,
            ..Self::default()
        }
    }

    fn move_dir(&mut self, direction: i32) {
        match direction {
            1 => self.x -= 1,
            2 => self.z -= 1,
            3 => self.x += 1,
            4 => self.z += 1,
            _ => {}
        }
    }
}

/// 对应 `rtg/tree/SkylightTracker.php`。
pub struct SkylightTracker {
    pub distance: i32,
    pub width: i32,
    direct_opacity: Vec<i32>,
    opacity: Vec<i32>,
    lowest: Vec<i32>,
    light_direction: Vec<i32>,
    relighting: Vec<bool>,
    back_track: Vec<i32>,
    trunk: Vec<bool>,
    pub base: Pos,
    pub tolerable_obstruction: i32,
    pub lighting_height: i32,
}

impl SkylightTracker {
    /// `distance` 是树的最大伸展半径（`furthestLikelyExtension()`）。
    pub fn new(distance: i32, base: Pos, world: &GenWorld) -> Self {
        Self::with_tolerance(distance, base, world, 4)
    }

    pub fn with_tolerance(
        distance: i32,
        base: Pos,
        world: &GenWorld,
        tolerable_obstruction: i32,
    ) -> Self {
        // PHP 自己再放 4 格余量，树冠边缘的光照才不会被截断
        let distance = distance + 4;
        let width = distance * 2 + 1;
        let cells = (width * width) as usize;
        let mut tracker = Self {
            distance,
            width,
            direct_opacity: vec![0; cells],
            opacity: vec![0; cells],
            lowest: vec![255; cells],
            light_direction: vec![0; cells],
            relighting: vec![false; cells],
            back_track: vec![0; cells],
            trunk: vec![false; cells],
            base,
            tolerable_obstruction,
            lighting_height: 50,
        };
        tracker.set_lighting(world);
        tracker.initial_spread();
        tracker
    }

    fn idx(&self, x: i32, z: i32) -> usize {
        (x * self.width + z) as usize
    }

    /// 对应 `baseAt`：网格坐标 → 世界坐标。
    pub fn base_at(&self, x: i32, z: i32) -> Pos {
        Pos::new(
            self.base.x - self.distance + x,
            self.base.y,
            self.base.z - self.distance + z,
        )
    }

    /// 对应 `legal()`：光扩散只在网格内部走（严格大于 0，边缘留一圈）。
    fn spot_legal(&self, spot: &Spotlight) -> bool {
        spot.x > 0 && spot.x < self.width && spot.z > 0 && spot.z < self.width
    }

    fn in_array(&self, x: i32, z: i32) -> bool {
        x > -1 && x < self.width && z > -1 && z < self.width
    }

    fn spot_opacity(&self, spot: &Spotlight) -> i32 {
        self.opacity[self.idx(spot.x, spot.z)]
    }

    fn set_spot_opacity(&mut self, spot: &Spotlight, value: i32) {
        let index = self.idx(spot.x, spot.z);
        self.opacity[index] = value;
    }

    fn spot_direct_opacity(&self, spot: &Spotlight) -> i32 {
        self.direct_opacity[self.idx(spot.x, spot.z)]
    }

    fn spot_direction(&self, spot: &Spotlight) -> i32 {
        self.light_direction[self.idx(spot.x, spot.z)]
    }

    fn set_spot_direction(&mut self, spot: &Spotlight, direction: i32) {
        let index = self.idx(spot.x, spot.z);
        self.light_direction[index] = direction;
    }

    fn spot_relighting(&self, spot: &Spotlight) -> bool {
        self.relighting[self.idx(spot.x, spot.z)]
    }

    fn set_spot_relighting(&mut self, spot: &Spotlight, value: bool) {
        let index = self.idx(spot.x, spot.z);
        self.relighting[index] = value;
    }

    fn spot_back_track(&self, spot: &Spotlight) -> i32 {
        self.back_track[self.idx(spot.x, spot.z)]
    }

    fn set_spot_back_track(&mut self, spot: &mut Spotlight, coming_from: i32) {
        let index = self.idx(spot.x, spot.z);
        self.back_track[index] = coming_from;
        spot.total_back_tracks += 1;
    }

    fn clear_spot_back_track(&mut self, spot: &mut Spotlight) {
        let index = self.idx(spot.x, spot.z);
        self.back_track[index] = 0;
        spot.total_back_tracks -= 1;
    }

    fn spot_trunk(&self, spot: &Spotlight) -> bool {
        self.trunk[self.idx(spot.x, spot.z)]
    }

    fn set_spot_trunk(&mut self, spot: &Spotlight, value: bool) {
        let index = self.idx(spot.x, spot.z);
        self.trunk[index] = value;
    }

    fn is_log(state: i32) -> bool {
        matches!(state >> 4, 17 | 162)
    }

    /// 对应 `setLighting`：从每列顶部往下扫，算出这一列天然的遮光。
    fn set_lighting(&mut self, world: &GenWorld) {
        for x in 0..self.width {
            for z in 0..self.width {
                let column_base = self.base_at(x, z);
                let mut obstruction = 0;
                if world.is_block_loaded(column_base.x, column_base.z) {
                    let mut up = 0;
                    // 先穿过顶部连续的不透明块，碰到原木（且离地 5 格以上）说明已经长在树里
                    while up < self.lighting_height {
                        let examined =
                            world.block_state(column_base.x, column_base.y + up, column_base.z);
                        if !block_rules::is_full_block(examined) {
                            break;
                        }
                        if Self::is_log(examined) && up > 5 {
                            obstruction = 255;
                            self.set_spot_trunk(&Spotlight::at(x, z), true);
                            break;
                        }
                        up += 1;
                    }
                    // 再往下累加透光损失，超过 14 就是死黑
                    while up < self.lighting_height {
                        let examined =
                            world.block_state(column_base.x, column_base.y + up, column_base.z);
                        if block_rules::is_full_block(examined) {
                            obstruction = 255;
                            break;
                        }
                        obstruction += block_rules::light_opacity(examined);
                        if obstruction > 14 {
                            break;
                        }
                        up += 1;
                    }
                }
                let index = self.idx(x, z);
                self.direct_opacity[index] = obstruction;
            }
        }
    }

    /// 对应 `initialSpread`：把直接遮光抄到 `opacity`，然后从每格扩散一遍。
    fn initial_spread(&mut self) {
        self.opacity.copy_from_slice(&self.direct_opacity);
        let mut spot = Spotlight::default();
        for x in 0..self.width {
            for z in 0..self.width {
                spot.x = x;
                spot.z = z;
                self.spread(&mut spot);
            }
        }
    }

    /// 对应 `spread`：向四个邻居扩散一层，比邻居当前值亮就更新并继续。
    fn spread(&mut self, spot: &mut Spotlight) {
        if self.spot_opacity(spot) > self.spot_direct_opacity(spot) {
            let direct = self.spot_direct_opacity(spot);
            self.set_spot_opacity(spot, direct);
            self.set_spot_direction(spot, 0);
        }
        let light = self.spot_opacity(spot);
        let old_x = spot.x;
        let old_z = spot.z;
        let dimmed = light + 1;
        if dimmed > 15 {
            return;
        }

        spot.x -= 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && !self.spot_trunk(spot)
            && self.spot_opacity(spot) > dimmed
        {
            self.set_spot_opacity(spot, dimmed);
            self.set_spot_direction(spot, 3);
            self.spread(spot);
        }
        spot.x += 1;

        spot.z -= 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && !self.spot_trunk(spot)
            && self.spot_opacity(spot) > dimmed
        {
            self.set_spot_opacity(spot, dimmed);
            self.set_spot_direction(spot, 4);
            self.spread(spot);
        }
        spot.z += 1;

        spot.x += 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && !self.spot_trunk(spot)
            && self.spot_opacity(spot) > dimmed
        {
            self.set_spot_opacity(spot, dimmed);
            self.set_spot_direction(spot, 1);
            self.spread(spot);
        }
        spot.x -= 1;

        spot.z += 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && !self.spot_trunk(spot)
            && self.spot_opacity(spot) > dimmed
        {
            self.set_spot_opacity(spot, dimmed);
            self.set_spot_direction(spot, 2);
            self.spread(spot);
        }
        spot.z -= 1;

        debug_assert_eq!(old_x, spot.x, "spread 必须原路返回");
        debug_assert_eq!(old_z, spot.z, "spread 必须原路返回");
    }

    /// 对应 `darken`：把"被新方块挡住"的那一片标成待重算。
    fn darken(&mut self, spot: &mut Spotlight, coming_from: i32) {
        self.set_spot_relighting(spot, true);
        self.set_spot_back_track(spot, coming_from);
        let direct = self.spot_direct_opacity(spot);
        self.set_spot_opacity(spot, direct);
        if self.spot_opacity(spot) > spot.passed_opacity {
            let passed = spot.passed_opacity;
            self.set_spot_opacity(spot, passed);
        }

        spot.x -= 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && self.spot_direction(spot) == 3
            && !self.spot_trunk(spot)
        {
            spot.passed_opacity += 1;
            self.darken(spot, 3);
            spot.passed_opacity -= 1;
        }
        spot.x += 1;

        spot.z -= 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && self.spot_direction(spot) == 4
            && !self.spot_trunk(spot)
        {
            spot.passed_opacity += 1;
            self.darken(spot, 4);
            spot.passed_opacity -= 1;
        }
        spot.z += 1;

        spot.x += 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && self.spot_direction(spot) == 1
            && !self.spot_trunk(spot)
        {
            spot.passed_opacity += 1;
            self.darken(spot, 1);
            spot.passed_opacity -= 1;
        }
        spot.x -= 1;

        spot.z += 1;
        if self.spot_legal(spot)
            && !self.spot_relighting(spot)
            && self.spot_direction(spot) == 2
            && !self.spot_trunk(spot)
        {
            spot.passed_opacity += 1;
            self.darken(spot, 2);
            spot.passed_opacity -= 1;
        }
        spot.z -= 1;
    }

    /// 对应 `relight`：重算完暗区后重新点亮，并回填 `opacity`。
    fn relight(&mut self, spot: &mut Spotlight, coming_from: i32) {
        spot.x -= 1;
        if self.spot_legal(spot) && self.spot_relighting(spot) && self.spot_direction(spot) == 3 {
            spot.passed_opacity += 1;
            self.relight(spot, 3);
            spot.passed_opacity -= 1;
        }
        spot.x += 1;

        spot.z -= 1;
        if self.spot_legal(spot) && self.spot_relighting(spot) && self.spot_direction(spot) == 4 {
            spot.passed_opacity += 1;
            self.relight(spot, 4);
            spot.passed_opacity -= 1;
        }
        spot.z += 1;

        spot.x += 1;
        if self.spot_legal(spot) && self.spot_relighting(spot) && self.spot_direction(spot) == 1 {
            spot.passed_opacity += 1;
            self.relight(spot, 1);
            spot.passed_opacity -= 1;
        }
        spot.x -= 1;

        spot.z += 1;
        if self.spot_legal(spot) && self.spot_relighting(spot) && self.spot_direction(spot) == 2 {
            spot.passed_opacity += 1;
            self.relight(spot, 2);
            spot.passed_opacity -= 1;
        }
        spot.z -= 1;

        self.set_spot_relighting(spot, false);
        self.spread_from(spot, 3);
        self.spread_from(spot, 4);
        self.spread_from(spot, 1);
        self.spread_from(spot, 2);
        self.spread(spot);

        if self.spot_opacity(spot) >= self.spot_direct_opacity(spot) {
            let direct = self.spot_direct_opacity(spot);
            self.set_spot_direction(spot, 0);
            self.set_spot_opacity(spot, direct);
        }
        if self.spot_opacity(spot) >= spot.passed_opacity {
            let passed = spot.passed_opacity;
            self.set_spot_direction(spot, coming_from);
            self.set_spot_opacity(spot, passed);
        }
    }

    fn spread_from(&mut self, spot: &mut Spotlight, tested: i32) {
        spot.move_dir(tested);
        if self.spot_legal(spot) {
            self.spread(spot);
        }
        spot.move_dir(reverse(tested));
    }

    fn check_backtracked_opacity(&mut self, spot: &mut Spotlight) {
        self.check_opacity_towards(spot, 3);
        self.check_opacity_towards(spot, 4);
        self.check_opacity_towards(spot, 1);
        self.check_opacity_towards(spot, 2);
        if !self.spot_trunk(spot) {
            let opacity = self.spot_opacity(spot);
            if opacity > spot.worst_opacity {
                spot.worst_opacity = opacity;
            }
        }
    }

    fn check_opacity_towards(&mut self, spot: &mut Spotlight, towards: i32) {
        spot.move_dir(towards);
        if self.spot_legal(spot) && self.spot_back_track(spot) == reverse(towards) {
            self.check_backtracked_opacity(spot);
            self.clear_spot_back_track(spot);
        }
        spot.move_dir(reverse(towards));
    }

    /// 对应 `testPlace`：试放一块，超预算就回滚并返回 false。
    pub fn test_place(
        &mut self,
        world: &mut GenWorld,
        location: Pos,
        placed: i32,
        generate_flags: i32,
    ) -> bool {
        let base_x = location.x - self.base.x + self.distance;
        let base_z = location.z - self.base.z + self.distance;
        if !self.in_array(base_x, base_z) {
            return true;
        }

        let mut spot = Spotlight::at(base_x, base_z);
        let index = self.idx(base_x, base_z);
        let old_direct = self.direct_opacity[index];
        self.direct_opacity[index] =
            (self.direct_opacity[index] + block_rules::light_opacity(placed)).min(255);

        // 这一列已经能被光照到、而且不是树干——直接放，不需要重算
        if self.light_direction[index] != 0 && !self.trunk[index] {
            self.commit(world, location, placed, generate_flags, index);
            return true;
        }

        spot.passed_opacity = self.direct_opacity[index];
        self.set_spot_opacity(&spot, self.direct_opacity[index]);
        self.darken(&mut spot, 0);
        self.relight(&mut spot, 0);
        spot.worst_opacity = 0;
        self.check_backtracked_opacity(&mut spot);
        self.clear_spot_back_track(&mut spot);

        if spot.worst_opacity <= self.tolerable_obstruction {
            self.commit(world, location, placed, generate_flags, index);
            return true;
        }

        // 超预算：撤回 directOpacity，让光重新扩散回原样
        self.direct_opacity[index] = old_direct;
        self.light_direction[index] = 0;
        self.spread(&mut spot);
        false
    }

    fn commit(
        &mut self,
        world: &mut GenWorld,
        location: Pos,
        placed: i32,
        generate_flags: i32,
        index: usize,
    ) {
        let _ = generate_flags;
        world.set_block_state(location.x, location.y, location.z, placed);
        if self.lowest[index] > location.y {
            self.lowest[index] = location.y;
        }
    }

    /// 对应 `testTrunk`：树干第一次占据这一列时，那一列从此不透光。
    ///
    /// PHP 里失败分支还会调 `chooseLight()`，但那个方法算出的方向最后没有写回
    /// 任何字段（纯读死代码），这里直接省掉。
    pub fn test_trunk(
        &mut self,
        world: &mut GenWorld,
        location: Pos,
        placed: i32,
        generate_flags: i32,
    ) -> bool {
        let base_x = location.x - self.base.x + self.distance;
        let base_z = location.z - self.base.z + self.distance;
        let mut place = Spotlight::at(base_x, base_z);
        if self.spot_trunk(&place) {
            return self.test_place(world, location, placed, generate_flags);
        }

        self.set_spot_trunk(&place, true);
        let successful = self.test_place(world, location, placed, generate_flags);
        if !successful {
            self.set_spot_trunk(&place, false);
            self.spread(&mut place);
        } else {
            self.set_spot_opacity(&place, 255);
        }
        successful
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        // 铺一层草方块，y=63 是地面
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 2 << 4);
            }
        }
        world
    }

    #[test]
    fn open_ground_has_no_obstruction() {
        let world = flat_world();
        let tracker = SkylightTracker::new(5, Pos::new(8, 64, 8), &world);
        // 全开阔地：每列都是 0 遮光，且没有树干
        assert!(tracker.direct_opacity.iter().all(|&value| value == 0));
        assert!(tracker.opacity.iter().all(|&value| value == 0));
        assert!(tracker.trunk.iter().all(|&value| !value));
    }

    #[test]
    fn canopy_overhead_blocks_light() {
        let mut world = flat_world();
        // 头顶盖一层树叶，那一列应该变暗
        for x in 6..11 {
            for z in 6..11 {
                world.set_block_state(x, 66, z, 18 << 4);
            }
        }
        let tracker = SkylightTracker::new(5, Pos::new(8, 64, 8), &world);
        let center = tracker.idx(5 + 4, 5 + 4); // distance=9, 中心在 9
        let _ = center;
        assert!(
            tracker.direct_opacity.iter().any(|&value| value > 0),
            "树叶应该产生遮光"
        );
    }

    #[test]
    fn placing_leaves_updates_lowest_after_commit() {
        let mut world = flat_world();
        let mut tracker = SkylightTracker::new(1, Pos::new(8, 64, 8), &world);
        let pos = Pos::new(8, 66, 8);
        let placed = 18 << 4 | 8;
        // 开阔地上放树叶一定能过（遮光远低于 4）
        assert!(tracker.test_place(&mut world, pos, placed, 19));
        assert_eq!(world.block_state(pos.x, pos.y, pos.z), placed);
        let index = tracker.idx(
            pos.x - tracker.base.x + tracker.distance,
            pos.z - tracker.base.z + tracker.distance,
        );
        assert_eq!(tracker.lowest[index], 66);
    }
}
