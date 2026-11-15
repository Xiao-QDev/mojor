//! 对应 `rtg/tree/{BranchVector,FractionalBlockPos,RTGTreeBranch}.php`。
//!
//! RTG 的树不是"每层画个圆"，而是让树枝沿着一个方向向量一格一格往外爬。
//! 这三个类型就是那套爬行机制：
//!
//! - `BranchVector`：单位化的方向（三个分量里绝对值最大的那个归一成 ±1）
//! - `FractionalPos`：带小数的位置，整数格由它 floor 出来
//! - `TreeBranch`：一段有剩余长度的树枝，每次"爬到下一个整数格"就减一点长度

use crate::level::generator::population::Pos;

/// 对应 `rtg/tree/BranchVector.php`。
#[derive(Clone, Debug)]
pub struct BranchVector {
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    pub length: f64,
}

impl BranchVector {
    /// 对应 `constructBranchVector2`：`horizontalDirection` 是弧度，`verticalShift` 是爬升。
    ///
    /// 归一化的分母是三个分量绝对值的最大值——所以每个分量都在 `[-1, 1]`，
    /// 且至少有一个方向会正好 +1 或 -1，保证"沿主方向每步走一格"。
    pub fn new(horizontal_direction: f64, vertical_shift: f64) -> Self {
        let trial_dx = horizontal_direction.cos();
        let trial_dz = horizontal_direction.sin();
        let trial_dy = vertical_shift;
        let longest = trial_dx.abs().max(trial_dy.abs()).max(trial_dz.abs());
        let dx = trial_dx / longest;
        let dy = trial_dy / longest;
        let dz = trial_dz / longest;
        Self {
            dx,
            dy,
            dz,
            length: (dx * dx + dy * dy + dz * dz).sqrt(),
        }
    }

    /// 对应 `move`：整段方向向量走一次。
    pub fn move_by(&self, moved: &mut FractionalPos) {
        moved.x += self.dx;
        moved.y += self.dy;
        moved.z += self.dz;
    }

    /// 对应 `moveFractionally`：走 `proportion` 段。
    pub fn move_fractionally(&self, moved: &mut FractionalPos, proportion: f64) {
        moved.x += self.dx * proportion;
        moved.y += self.dy * proportion;
        moved.z += self.dz * proportion;
    }

    /// 对应 `reposition`：从 `pos` 沿方向走 `distance` 后落到哪个格子。
    pub fn reposition(&self, pos: Pos, distance: f64) -> Pos {
        Pos::new(
            pos.x + (self.dx * distance + 0.5).floor() as i32,
            pos.y + (self.dy * distance + 0.5).floor() as i32,
            pos.z + (self.dz * distance + 0.5).floor() as i32,
        )
    }
}

/// 对应 `rtg/tree/FractionalBlockPos.php`。
#[derive(Clone, Copy, Debug)]
pub struct FractionalPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl FractionalPos {
    /// 从整数格构造：三个轴都 +0.5，落在方块中心。
    pub fn from_pos(pos: Pos) -> Self {
        Self {
            x: pos.x as f64 + 0.5,
            y: pos.y as f64 + 0.5,
            z: pos.z as f64 + 0.5,
        }
    }

    /// 对应 `new FractionalBlockPos($fractional)`：复制时**不再**加 0.5。
    pub fn copy(other: &Self) -> Self {
        *other
    }

    /// 对应 `location()`。注意 PHP 里 x/z 是 `floor`、y 是 `(int)` 截断，
    /// 负数时两者差一格——照抄，不然树枝落点会偏。
    pub fn location(&self) -> Pos {
        Pos::new(self.x.floor() as i32, self.y as i32, self.z.floor() as i32)
    }
}

/// 对应 `rtg/tree/RTGTreeBranch.php`。
#[derive(Clone, Debug)]
pub struct TreeBranch {
    pub direction: BranchVector,
    pub initial_length: f64,
    pub remaining_length: f64,
    pub initial_horizontal: f64,
    pub initial_vertical: f64,
    pub stage: i32,
    pub branch_location: FractionalPos,
}

impl TreeBranch {
    /// 对应 `constructRTGTreeBranch5`。
    pub fn new(horizontal: f64, vertical: f64, length: f64, stage: i32, branch_start: Pos) -> Self {
        Self {
            initial_length: length,
            remaining_length: length,
            initial_horizontal: horizontal,
            initial_vertical: vertical,
            stage,
            direction: BranchVector::new(horizontal, vertical),
            branch_location: FractionalPos::from_pos(branch_start),
        }
    }

    /// 对应 `constructRTGTreeBranch1`：复制一条树枝并重置剩余长度。
    pub fn copied(other: &Self) -> Self {
        Self {
            initial_length: other.initial_length,
            remaining_length: other.initial_length,
            initial_horizontal: other.initial_horizontal,
            initial_vertical: other.initial_vertical,
            stage: other.stage,
            direction: other.direction.clone(),
            branch_location: FractionalPos::copy(&other.branch_location),
        }
    }

    /// 对应 `moved`：按方向整段前进。
    pub fn moved(&mut self) -> Pos {
        self.direction.move_by(&mut self.branch_location);
        self.remaining_length -= self.direction.length;
        self.branch_location.location()
    }

    /// 对应 `movedOrthogonally`：只走到**下一个整数格**，保证每格都落一次方块。
    ///
    /// PHP 里 `$multiplier` 加了 `.0001` 只用于扣剩余长度，实际位移用的是重新算的
    /// `multiplierToNextBlock()`——这个不对称照抄，改了就偏。
    pub fn moved_orthogonally(&mut self) -> Pos {
        let multiplier = self.multiplier_to_next_block() + 0.0001;
        let step = self.multiplier_to_next_block();
        self.direction
            .move_fractionally(&mut self.branch_location, step);
        self.remaining_length -= multiplier.abs() / self.direction.length;
        self.branch_location.location()
    }

    /// 对应 `multiplierToNextBlock`。
    pub fn multiplier_to_next_block(&self) -> f64 {
        let x_mult = self.multiplier_to_next_integer(self.branch_location.x, self.direction.dx);
        let y_mult = self.multiplier_to_next_integer(self.branch_location.y, self.direction.dy);
        let z_mult = self.multiplier_to_next_integer(self.branch_location.z, self.direction.dz);
        let result = y_mult.min(z_mult).min(x_mult);
        debug_assert!(result >= 0.0, "RTG terrain invariant: 负的下一格倍数");
        result
    }

    /// 对应 `multiplierToNextInteger`：沿 `change` 方向走几步会跨过下一个整数。
    fn multiplier_to_next_integer(&self, number: f64, change: f64) -> f64 {
        if change.abs() < 0.00001 {
            return 100000.0;
        }
        let to_go = if change < 0.0 {
            let mut to_go = (number.floor() - number) - 0.000001;
            if to_go > -0.00001 {
                to_go = -1.0;
            }
            to_go
        } else {
            let mut to_go = (number.ceil() - number) + 0.000001;
            if to_go < 0.00001 {
                to_go = 1.0;
            }
            to_go
        };
        to_go / change
    }

    pub fn location(&self) -> Pos {
        self.branch_location.location()
    }

    pub fn not_done(&self) -> bool {
        self.remaining_length > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_vector_is_normalized_on_longest_axis() {
        // 正东：dx 归一成 1，dy/dz 为 0
        let east = BranchVector::new(0.0, 0.0);
        assert!((east.dx - 1.0).abs() < 1e-12);
        assert!(east.dz.abs() < 1e-12);
        assert!((east.length - 1.0).abs() < 1e-12);

        // 45 度 + 垂直 1：最长分量是 dy=1，dx=dz=√2/2
        let diagonal = BranchVector::new(std::f64::consts::FRAC_PI_4, 1.0);
        assert!((diagonal.dy - 1.0).abs() < 1e-12);
        assert!((diagonal.dx - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
    }

    #[test]
    fn fractional_location_floors_xz_and_truncates_y() {
        // x/z 用 floor（-1.5 → -2），y 用截断（-1.5 → -1）
        let fractional = FractionalPos {
            x: -1.5,
            y: -1.5,
            z: 2.7,
        };
        assert_eq!(fractional.location(), Pos::new(-2, -1, 2));
    }

    #[test]
    fn fractional_pos_from_block_adds_half() {
        let pos = FractionalPos::from_pos(Pos::new(3, 64, -2));
        assert_eq!(pos.x, 3.5);
        assert_eq!(pos.y, 64.5);
        assert_eq!(pos.z, -1.5);
        assert_eq!(pos.location(), Pos::new(3, 64, -2));
    }

    #[test]
    fn branch_steps_one_block_at_a_time() {
        // `(0, 2)` 归一化后是 (0.5, 1, 0)：不是纯竖直，但每一步只跨一个轴一格
        let mut branch = TreeBranch::new(0.0, 2.0, 5.0, 1, Pos::new(0, 64, 0));
        let mut last = branch.location();
        let mut steps = 0;
        while branch.not_done() {
            let next = branch.moved_orthogonally();
            let changed = i32::from(next.x != last.x)
                + i32::from(next.y != last.y)
                + i32::from(next.z != last.z);
            assert_eq!(changed, 1, "一次只能跨一个轴：{last:?} -> {next:?}");
            let distance =
                (next.x - last.x).abs() + (next.y - last.y).abs() + (next.z - last.z).abs();
            assert_eq!(distance, 1, "每次只走一格：{last:?} -> {next:?}");
            last = next;
            steps += 1;
            assert!(steps < 100, "树枝没有收敛");
        }
        assert!(steps >= 4, "5 格长的树枝至少走 4 步，实际 {steps}");
    }
}
