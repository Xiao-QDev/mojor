//! 对应 `rtg/feature/{WorldGenTrees,WorldGenBigTree,WorldGenBirchTree,WorldGenTaiga1,
//! WorldGenTaiga2,WorldGenSwamp,WorldGenCanopyTree,WorldGenMegaPineTree,
//! WorldGenMegaJungle,WorldGenSavannaTree,WorldGenShrub,WorldGenHugeTrees}.php`：
//! 原版 Minecraft 1.12 的树木生成器。
//!
//! 这些类只被 `VanillaBiomeDecorator::treeFor` 用来复刻原版 biome 的树形，
//! 与 `tree/vanilla.rs` 里的 RTG 树链（`DecoVariableTree` 的兜底路径）是两条线。
//!
//! 三个必须守住的约定：
//! - 随机数消耗顺序与 PHP 完全一致（`treeFor` 里构造树时消耗的随机数也在
//!   [`generate_vanilla_named_tree`] 的对应分支里消耗）。
//! - 浮点每一步都按 PHP 的 `F::f(...)` 嵌套顺序做 float32 截断。
//! - 世界高度是 128（`CHUNK_HEIGHT`），PHP 里写死 256 的边界判断都换成 [`HEIGHT`]。
//!
//! `setDecorationDefaults()` 只有 `WorldGenBigTree` 有实际内容（把
//! `leafDistanceLimit` 从 4 改成 5），这里直接在构造函数里取 5；
//! `generateSaplings()` 一律为空实现（本移植没有树苗）。

use crate::level::format::chunk::CHUNK_HEIGHT;
use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::carver::math_helper;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_vanilla_tree;
use crate::level::generator::realistic_terrain::tree::TreeMaterials;
use crate::level::generator::world::GenWorld;

/// PHP 里的 256（世界高度）。
const HEIGHT: i32 = CHUNK_HEIGHT as i32;

// ---------------------------------------------------------------------------
// 各方块的完整 state（`id << 4 | meta`），数值由 PHP 的 `withProperty` 链推出。
// ---------------------------------------------------------------------------

/// `woodState(3, false)`。
const JUNGLE_LOG: i32 = (17 << 4) | 3;
/// `woodState(3, true)`。
const JUNGLE_LEAVES: i32 = (18 << 4) | 3 | 8;
/// `withProperty(withProperty(296, 'variant', 0), 'check_decay', false)`。
const OAK_LEAVES: i32 = 18 << 4;
/// `woodState(0, true)`：丛林灌木的树叶。
const OAK_LEAVES_DECAY: i32 = (18 << 4) | 8;
/// `withProperty(272, 'variant', 1)`。
const SPRUCE_LOG: i32 = (17 << 4) | 1;
/// `withProperty(withProperty(296, 'variant', 1), 'check_decay', false)`。
const SPRUCE_LEAVES: i32 = (18 << 4) | 1;
/// `withProperty(272, 'variant', 2)`。
const BIRCH_LOG: i32 = (17 << 4) | 2;
/// `withProperty(withProperty(296, 'variant', 2), 'check_decay', false)`。
const BIRCH_LEAVES: i32 = (18 << 4) | 2;
/// `withProperty(2592, 'variant', 5)`。
const DARK_OAK_LOG: i32 = (162 << 4) | 1;
/// `withProperty(withProperty(2584, 'variant', 5), 'check_decay', false)`。
const DARK_OAK_LEAVES: i32 = (161 << 4) | 1;
/// `withProperty(2592, 'variant', 4)`。
const ACACIA_LOG: i32 = 162 << 4;
/// `withProperty(withProperty(2584, 'variant', 4), 'check_decay', false)`。
const ACACIA_LEAVES: i32 = 161 << 4;

/// `Facing::horizontal()` = NORTH, EAST, SOUTH, WEST 的 (dx, dz)。
const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
/// 同一顺序的 `getOpposite()` 偏移。
const OPPOSITE: [(i32, i32); 4] = [(0, 1), (-1, 0), (0, -1), (1, 0)];
/// 同一顺序的 `getHorizontalIndex()`（可可果 meta 的低两位）。
const HORIZONTAL_META: [i32; 4] = [2, 3, 0, 1];

/// 对应 `WorldGenAbstractTree::canGrowInto`。
fn can_grow_into(block_id: i32) -> bool {
    matches!(
        block_rules::material(block_rules::default_state(block_id)),
        block_rules::AIR | block_rules::LEAVES
    ) || matches!(block_id, 2 | 3 | 17 | 162 | 6 | 106)
}

/// 对应 `WorldGenAbstractTree::setDirtAt`。
fn set_dirt_at(world: &mut GenWorld, pos: Pos) {
    // Java 侧灰化土就是 dirt 的 variant 2；落地后是 native 243，同样保持原样
    if !matches!(world.block_state(pos.x, pos.y, pos.z) >> 4, 3 | 243) {
        world.set_block_state(pos.x, pos.y, pos.z, 48);
    }
}

/// 对应 `WorldGenTrees::addVine`。
fn add_vine(world: &mut GenWorld, pos: Pos, face: &str) {
    world.set_block_state(pos.x, pos.y, pos.z, block_rules::vine_with_face(face));
}

/// 对应 `WorldGenTrees::addHangingVine`：先挂一格，再往下最多挂 4 格。
fn add_hanging_vine(world: &mut GenWorld, pos: Pos, face: &str) {
    add_vine(world, pos, face);
    let mut below = pos.down();
    let mut remaining = 4;
    while block_rules::material(world.block_state(below.x, below.y, below.z)) == block_rules::AIR
        && remaining > 0
    {
        add_vine(world, below, face);
        below = below.down();
        remaining -= 1;
    }
}

// ---------------------------------------------------------------------------
// WorldGenTrees：带自定义树高的橡树/丛林树。
// ---------------------------------------------------------------------------

/// 对应 `new WorldGenTrees(notify, minTreeHeightIn, woodMeta, leavesMeta, growVines)`。
///
/// 橡树走 `vanilla::generate_vanilla_tree`（固定 `4 + rand(3)`）；
/// 丛林树用这个带高度参数的版本（`growVines=true`，还会长可可果）。
fn generate_trees(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    min_tree_height: i32,
    log: i32,
    leaves: i32,
    vines_grow: bool,
) -> bool {
    let height = rand.next_int_bound(3) + min_tree_height;
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y == pos.y {
            0
        } else if y >= pos.y + 1 + height - 2 {
            2
        } else {
            1
        };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT || !can_grow_into(world.block_state(x, y, z) >> 4) {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 60 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    // 树冠：最底下三层，半径从 2 收到 0。
    for layer in pos.y - 3 + height..=pos.y + height {
        let from_top = layer - (pos.y + height);
        let radius = 1 - from_top / 2;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius
                    || dz.abs() != radius
                    || (rand.next_int_bound(2) != 0 && from_top != 0)
                {
                    if matches!(
                        block_rules::material(world.block_state(x, layer, z)),
                        block_rules::AIR | block_rules::LEAVES | block_rules::VINE
                    ) {
                        world.set_block_state(x, layer, z, leaves);
                    }
                }
            }
        }
    }

    // 树干 + 贴树干的藤蔓。
    for step in 0..height {
        let trunk = pos.up_n(step);
        if matches!(
            block_rules::material(world.block_state(trunk.x, trunk.y, trunk.z)),
            block_rules::AIR | block_rules::LEAVES | block_rules::VINE
        ) {
            world.set_block_state(trunk.x, trunk.y, trunk.z, log);
            if vines_grow && step > 0 {
                if rand.next_int_bound(3) > 0 && world.is_air(pos.x - 1, pos.y + step, pos.z) {
                    add_vine(world, Pos::new(pos.x - 1, pos.y + step, pos.z), "east");
                }
                if rand.next_int_bound(3) > 0 && world.is_air(pos.x + 1, pos.y + step, pos.z) {
                    add_vine(world, Pos::new(pos.x + 1, pos.y + step, pos.z), "west");
                }
                if rand.next_int_bound(3) > 0 && world.is_air(pos.x, pos.y + step, pos.z - 1) {
                    add_vine(world, Pos::new(pos.x, pos.y + step, pos.z - 1), "south");
                }
                if rand.next_int_bound(3) > 0 && world.is_air(pos.x, pos.y + step, pos.z + 1) {
                    add_vine(world, Pos::new(pos.x, pos.y + step, pos.z + 1), "north");
                }
            }
        }
    }

    // 树冠侧面的垂藤。
    if vines_grow {
        for layer in pos.y - 3 + height..=pos.y + height {
            let from_top = layer - (pos.y + height);
            let radius = 2 - from_top / 2;
            for x in pos.x - radius..=pos.x + radius {
                for z in pos.z - radius..=pos.z + radius {
                    if block_rules::material(world.block_state(x, layer, z)) != block_rules::LEAVES {
                        continue;
                    }
                    let west = Pos::new(x - 1, layer, z);
                    let east = Pos::new(x + 1, layer, z);
                    let north = Pos::new(x, layer, z - 1);
                    let south = Pos::new(x, layer, z + 1);
                    if rand.next_int_bound(4) == 0
                        && block_rules::material(world.block_state(west.x, west.y, west.z))
                            == block_rules::AIR
                    {
                        add_hanging_vine(world, west, "east");
                    }
                    if rand.next_int_bound(4) == 0
                        && block_rules::material(world.block_state(east.x, east.y, east.z))
                            == block_rules::AIR
                    {
                        add_hanging_vine(world, east, "west");
                    }
                    if rand.next_int_bound(4) == 0
                        && block_rules::material(world.block_state(north.x, north.y, north.z))
                            == block_rules::AIR
                    {
                        add_hanging_vine(world, north, "south");
                    }
                    if rand.next_int_bound(4) == 0
                        && block_rules::material(world.block_state(south.x, south.y, south.z))
                            == block_rules::AIR
                    {
                        add_hanging_vine(world, south, "north");
                    }
                }
            }
        }
    }

    // 可可果：1/5 概率，两层，按 Facing::horizontal() 顺序抽。
    if rand.next_int_bound(5) == 0 && height > 5 {
        for layer in 0..2 {
            for index in 0..4 {
                if rand.next_int_bound(4 - layer) == 0 {
                    let age = rand.next_int_bound(3);
                    let (ox, oz) = OPPOSITE[index];
                    world.set_block_state(
                        pos.x + ox,
                        pos.y + (height - 5) + layer,
                        pos.z + oz,
                        (127 << 4) | (age << 2) | HORIZONTAL_META[index],
                    );
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// WorldGenBigTree：巨橡树。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenBigTree`（`heightAttenuation=0.618`、`branchSlope=0.381`、
/// `scaleWidth=1.0`、`leafDensity=1.0`、`trunkSize=1`、`heightLimitLimit=12`）。
struct BigTree {
    base_pos: Pos,
    height_limit: i32,
    height: i32,
    leaf_distance_limit: i32,
    /// 每片树叶的落点 + 树枝从树干哪个高度分出去。
    foliage: Vec<(Pos, i32)>,
}

impl BigTree {
    const HEIGHT_ATTENUATION: f64 = 0.618;
    const BRANCH_SLOPE: f64 = 0.381;
    const SCALE_WIDTH: f64 = 1.0;
    const LEAF_DENSITY: f64 = 1.0;
    const HEIGHT_LIMIT_LIMIT: i32 = 12;

    /// 对应 `setDecorationDefaults()`：`leafDistanceLimit = 5`。
    fn new(pos: Pos) -> Self {
        Self {
            base_pos: pos,
            height_limit: 0,
            height: 0,
            leaf_distance_limit: 5,
            foliage: Vec::new(),
        }
    }

    /// 对应 `generateLeafNodeList`。
    fn generate_leaf_node_list(&mut self, world: &GenWorld, rand: &mut JavaRandom) {
        self.height = (self.height_limit as f64 * Self::HEIGHT_ATTENUATION) as i32;
        if self.height >= self.height_limit {
            self.height = self.height_limit - 1;
        }
        let mut count =
            (1.382 + ((Self::LEAF_DENSITY * self.height_limit as f64) / 13.0).powf(2.0)) as i32;
        if count < 1 {
            count = 1;
        }
        let top = self.base_pos.y + self.height;
        let mut layer = self.height_limit - self.leaf_distance_limit;
        self.foliage.clear();
        self.foliage.push((self.base_pos.up_n(layer), top));
        while layer >= 0 {
            let size = self.layer_size(layer);
            if size >= 0.0 {
                for _ in 0..count {
                    let d0 = Self::SCALE_WIDTH * size * (rand.next_float() as f64 + 0.328);
                    let d1 = f(rand.next_float() as f64 * 2.0) * std::f64::consts::PI;
                    let d2 = d0 * d1.sin() + 0.5;
                    let d3 = d0 * d1.cos() + 0.5;
                    let leaf = self.base_pos.add_f64(d2, (layer - 1) as f64, d3);
                    let leaf_top = leaf.up_n(self.leaf_distance_limit);
                    if self.check_block_line(world, leaf, leaf_top) != -1 {
                        continue;
                    }
                    let dx = self.base_pos.x - leaf.x;
                    let dz = self.base_pos.z - leaf.z;
                    let d4 = leaf.y as f64
                        - (((dx * dx + dz * dz) as f64).sqrt() * Self::BRANCH_SLOPE);
                    let branch_base = if d4 > top as f64 { top } else { d4 as i32 };
                    let branch = Pos::new(self.base_pos.x, branch_base, self.base_pos.z);
                    if self.check_block_line(world, branch, leaf) == -1 {
                        self.foliage.push((leaf, branch.y));
                    }
                }
            }
            layer -= 1;
        }
    }

    /// 对应 `crosSection`：一层圆形的树叶截面。
    fn cross_section(&self, world: &mut GenWorld, pos: Pos, radius: f64, leaves: i32) {
        let extent = (radius + 0.618) as i32;
        for dx in -extent..=extent {
            for dz in -extent..=extent {
                let a = dx.abs() as f64 + 0.5;
                let b = dz.abs() as f64 + 0.5;
                if a.powf(2.0) + b.powf(2.0) > f(radius * radius) {
                    continue;
                }
                let target = pos.add(dx, 0, dz);
                let material = block_rules::material(world.block_state(target.x, target.y, target.z));
                if matches!(material, block_rules::AIR | block_rules::LEAVES) {
                    world.set_block_state(target.x, target.y, target.z, leaves);
                }
            }
        }
    }

    /// 对应 `layerSize`：该高度处树冠的半径。
    fn layer_size(&self, y: i32) -> f64 {
        if f(y as f64) < f(f(self.height_limit as f64) * 0.30000001192092896) {
            return f(-1.0);
        }
        let half = f(f(self.height_limit as f64) / 2.0);
        let offset = f(half - f(y as f64));
        let square = f(f(half * half) - f(offset * offset));
        let mut radius = f(square.sqrt());
        if offset == 0.0 {
            radius = f(half);
        } else if offset.abs() >= half {
            return f(0.0);
        }
        f(radius * 0.5)
    }

    /// 对应 `leafSize`：树叶截面按距离叶尖的层数分 2/3 两种半径。
    fn leaf_size(&self, y: i32) -> f64 {
        if y >= 0 && y < self.leaf_distance_limit {
            f(if y != 0 && y != self.leaf_distance_limit - 1 {
                3.0
            } else {
                2.0
            })
        } else {
            f(-1.0)
        }
    }

    /// 对应 `generateLeafNode`。
    fn generate_leaf_node(&self, world: &mut GenWorld, pos: Pos) {
        for i in 0..self.leaf_distance_limit {
            self.cross_section(world, pos.up_n(i), self.leaf_size(i), OAK_LEAVES);
        }
    }

    /// 对应 `limb`：两点之间按步长铺一条原木（带 log_axis）。
    fn limb(&self, world: &mut GenWorld, from: Pos, to: Pos, log_id: i32) {
        let diff = to.add(-from.x, -from.y, -from.z);
        let count = greatest_distance(diff);
        let step_x = f(f(diff.x as f64) / f(count as f64));
        let step_y = f(f(diff.y as f64) / f(count as f64));
        let step_z = f(f(diff.z as f64) / f(count as f64));
        for j in 0..=count {
            let target = from.add_f64(
                f(0.5 + f(f(j as f64) * step_x)),
                f(0.5 + f(f(j as f64) * step_y)),
                f(0.5 + f(f(j as f64) * step_z)),
            );
            let axis = log_axis(from, target);
            let state = block_rules::with_property(
                block_rules::default_state(log_id),
                "log_axis",
                axis,
            )
            .expect("原木支持 log_axis 属性");
            world.set_block_state(target.x, target.y, target.z, state);
        }
    }

    /// 对应 `generateLeaves`。
    fn generate_leaves(&self, world: &mut GenWorld) {
        for &(pos, _) in &self.foliage {
            self.generate_leaf_node(world, pos);
        }
    }

    /// 对应 `leafNodeNeedsBase`。
    fn leaf_node_needs_base(&self, height: i32) -> bool {
        height as f64 >= self.height_limit as f64 * 0.2
    }

    /// 对应 `generateTrunk`。PHP 里 `trunkSize` 固定为 1，`trunkSize==2` 的分支不执行。
    fn generate_trunk(&self, world: &mut GenWorld) {
        let base = self.base_pos;
        let top = self.base_pos.up_n(self.height);
        self.limb(world, base, top, 17);
    }

    /// 对应 `generateLeafNodeBases`。
    fn generate_leaf_node_bases(&self, world: &mut GenWorld) {
        for &(leaf, branch_base) in &self.foliage {
            let base = Pos::new(self.base_pos.x, branch_base, self.base_pos.z);
            if base != leaf && self.leaf_node_needs_base(branch_base - self.base_pos.y) {
                self.limb(world, base, leaf, 17);
            }
        }
    }

    /// 对应 `checkBlockLine`：返回第一个长不进去的下标，全程通畅返回 -1。
    fn check_block_line(&self, world: &GenWorld, from: Pos, to: Pos) -> i32 {
        let diff = to.add(-from.x, -from.y, -from.z);
        let count = greatest_distance(diff);
        let step_x = f(f(diff.x as f64) / f(count as f64));
        let step_y = f(f(diff.y as f64) / f(count as f64));
        let step_z = f(f(diff.z as f64) / f(count as f64));
        if count == 0 {
            return -1;
        }
        for j in 0..=count {
            let target = from.add_f64(
                f(0.5 + f(f(j as f64) * step_x)),
                f(0.5 + f(f(j as f64) * step_y)),
                f(0.5 + f(f(j as f64) * step_z)),
            );
            if !can_grow_into(world.block_state(target.x, target.y, target.z) >> 4) {
                return j;
            }
        }
        -1
    }

    /// 对应 `validTreeLocation`：地基要草/土/耕地，树顶空间不够时会压矮。
    fn valid_tree_location(&mut self, world: &GenWorld) -> bool {
        let ground = world.block_state(self.base_pos.x, self.base_pos.y - 1, self.base_pos.z) >> 4;
        if !matches!(ground, 3 | 2 | 60 | 243) {
            return false;
        }
        let blocked = self.check_block_line(
            world,
            self.base_pos,
            self.base_pos.up_n(self.height_limit - 1),
        );
        if blocked == -1 {
            return true;
        }
        if blocked < 6 {
            return false;
        }
        self.height_limit = blocked;
        true
    }
}

/// 对应 `WorldGenBigTree::generate`：自己的 `JavaRandom` 由调用者的 nextLong 播种。
fn generate_big_tree(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let mut local = JavaRandom::new(rand.next_long());
    let mut tree = BigTree::new(pos);
    tree.height_limit = 5 + local.next_int_bound(BigTree::HEIGHT_LIMIT_LIMIT);
    if !tree.valid_tree_location(world) {
        return false;
    }
    tree.generate_leaf_node_list(world, &mut local);
    tree.generate_leaves(world);
    tree.generate_trunk(world);
    tree.generate_leaf_node_bases(world);
    true
}

/// 对应 `getGreatestDistance`。
fn greatest_distance(pos: Pos) -> i32 {
    let x = pos.x.abs();
    let y = pos.y.abs();
    let z = pos.z.abs();
    if z > x && z > y {
        z
    } else if y > x {
        y
    } else {
        x
    }
}

/// 对应 `getLogAxis`：0 = 竖、4 = 东西向、8 = 南北向。
fn log_axis(from: Pos, to: Pos) -> i32 {
    let dx = (to.x - from.x).abs();
    let dz = (to.z - from.z).abs();
    let max = if dx > dz { dx } else { dz };
    if max > 0 {
        if dx == max {
            4
        } else if dz == max {
            8
        } else {
            0
        }
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// WorldGenBirchTree。
// ---------------------------------------------------------------------------

/// 对应 `new WorldGenBirchTree(notify, useExtraRandomHeight)`。
fn generate_birch(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    use_extra_random_height: bool,
) -> bool {
    let mut height = rand.next_int_bound(3) + 5;
    if use_extra_random_height {
        height += rand.next_int_bound(7);
    }
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y == pos.y {
            0
        } else if y >= pos.y + 1 + height - 2 {
            2
        } else {
            1
        };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT || !can_grow_into(world.block_state(x, y, z) >> 4) {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 60 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    // 白桦的树冠只有 AIR/LEAVES 能被覆盖（没有藤蔓）。
    for layer in pos.y - 3 + height..=pos.y + height {
        let from_top = layer - (pos.y + height);
        let radius = 1 - from_top / 2;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius
                    || dz.abs() != radius
                    || (rand.next_int_bound(2) != 0 && from_top != 0)
                {
                    if matches!(
                        block_rules::material(world.block_state(x, layer, z)),
                        block_rules::AIR | block_rules::LEAVES
                    ) {
                        world.set_block_state(x, layer, z, BIRCH_LEAVES);
                    }
                }
            }
        }
    }

    for step in 0..height {
        let trunk = pos.up_n(step);
        if matches!(
            block_rules::material(world.block_state(trunk.x, trunk.y, trunk.z)),
            block_rules::AIR | block_rules::LEAVES
        ) {
            world.set_block_state(trunk.x, trunk.y, trunk.z, BIRCH_LOG);
        }
    }
    true
}

// ---------------------------------------------------------------------------
// WorldGenTaiga1 / WorldGenTaiga2：云杉。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenTaiga1::generate`。
fn generate_taiga1(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let height = rand.next_int_bound(5) + 7;
    let trunk_start = height - rand.next_int_bound(2) - 3;
    let max_radius = height - trunk_start;
    let leaf_radius = 1 + rand.next_int_bound(max_radius + 1);
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y - pos.y < trunk_start {
            0
        } else {
            leaf_radius
        };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT || !can_grow_into(world.block_state(x, y, z) >> 4) {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    // 自顶向下铺树叶，半径先收后放。
    let mut radius = 0;
    let mut layer = pos.y + height;
    while layer >= pos.y + trunk_start {
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius || dz.abs() != radius || radius <= 0 {
                    let target = Pos::new(x, layer, z);
                    if !block_rules::is_full_block(
                        world.block_state(target.x, target.y, target.z),
                    ) {
                        world.set_block_state(target.x, target.y, target.z, SPRUCE_LEAVES);
                    }
                }
            }
        }
        if radius >= 1 && layer == pos.y + trunk_start + 1 {
            radius -= 1;
        } else if radius < leaf_radius {
            radius += 1;
        }
        layer -= 1;
    }

    for step in 0..height - 1 {
        let trunk = pos.up_n(step);
        if matches!(
            block_rules::material(world.block_state(trunk.x, trunk.y, trunk.z)),
            block_rules::AIR | block_rules::LEAVES
        ) {
            world.set_block_state(trunk.x, trunk.y, trunk.z, SPRUCE_LOG);
        }
    }
    true
}

/// 对应 `WorldGenTaiga2::generate`。
fn generate_taiga2(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let height = rand.next_int_bound(4) + 6;
    let trunk_top = 1 + rand.next_int_bound(2);
    let layers = height - trunk_top;
    let base_radius = 2 + rand.next_int_bound(2);
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y - pos.y < trunk_top { 0 } else { base_radius };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT {
                    valid = false;
                    break 'scan;
                }
                if !matches!(
                    block_rules::material(world.block_state(x, y, z)),
                    block_rules::AIR | block_rules::LEAVES
                ) {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 60 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    let mut radius = rand.next_int_bound(2);
    let mut grow_steps = 1;
    let mut shrink_from = 0;
    for step in 0..=layers {
        let layer = (pos.y + height) - step;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius || dz.abs() != radius || radius <= 0 {
                    let target = Pos::new(x, layer, z);
                    if !block_rules::is_full_block(
                        world.block_state(target.x, target.y, target.z),
                    ) {
                        world.set_block_state(target.x, target.y, target.z, SPRUCE_LEAVES);
                    }
                }
            }
        }
        if radius >= grow_steps {
            radius = shrink_from;
            shrink_from = 1;
            grow_steps += 1;
            if grow_steps > base_radius {
                grow_steps = base_radius;
            }
        } else {
            radius += 1;
        }
    }

    let trunk_cut = rand.next_int_bound(3);
    for step in 0..height - trunk_cut {
        let trunk = pos.up_n(step);
        if matches!(
            block_rules::material(world.block_state(trunk.x, trunk.y, trunk.z)),
            block_rules::AIR | block_rules::LEAVES
        ) {
            world.set_block_state(trunk.x, trunk.y, trunk.z, SPRUCE_LOG);
        }
    }
    true
}

// ---------------------------------------------------------------------------
// WorldGenSwamp：沼泽橡树（树干会先沉到水面以下）。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenSwamp::generate`。
fn generate_swamp(world: &mut GenWorld, rand: &mut JavaRandom, mut pos: Pos) -> bool {
    let height = rand.next_int_bound(4) + 5;
    while block_rules::material(world.block_state(pos.x, pos.y - 1, pos.z)) == block_rules::WATER {
        pos = pos.down();
    }
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y == pos.y {
            0
        } else if y >= pos.y + 1 + height - 2 {
            3
        } else {
            1
        };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT {
                    valid = false;
                    break 'scan;
                }
                let state = world.block_state(x, y, z);
                if matches!(
                    block_rules::material(state),
                    block_rules::AIR | block_rules::LEAVES
                ) {
                    continue;
                }
                let block = state >> 4;
                if block != 9 && block != 8 {
                    valid = false;
                    break 'scan;
                }
                if y > pos.y {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    for layer in pos.y - 3 + height..=pos.y + height {
        let from_top = layer - (pos.y + height);
        let radius = 2 - from_top / 2;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius
                    || dz.abs() != radius
                    || (rand.next_int_bound(2) != 0 && from_top != 0)
                {
                    let target = Pos::new(x, layer, z);
                    if !block_rules::is_full_block(world.block_state(target.x, target.y, target.z))
                    {
                        world.set_block_state(target.x, target.y, target.z, OAK_LEAVES);
                    }
                }
            }
        }
    }

    for step in 0..height {
        let trunk = pos.up_n(step);
        let state = world.block_state(trunk.x, trunk.y, trunk.z);
        let block = state >> 4;
        if matches!(
            block_rules::material(state),
            block_rules::AIR | block_rules::LEAVES
        ) || block == 8
            || block == 9
        {
            world.set_block_state(trunk.x, trunk.y, trunk.z, 17 << 4);
        }
    }

    // 树冠侧面最多垂 4 格的藤蔓。
    for layer in pos.y - 3 + height..=pos.y + height {
        let from_top = layer - (pos.y + height);
        let radius = 2 - from_top / 2;
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if block_rules::material(world.block_state(x, layer, z)) != block_rules::LEAVES {
                    continue;
                }
                let west = Pos::new(x - 1, layer, z);
                let east = Pos::new(x + 1, layer, z);
                let north = Pos::new(x, layer, z - 1);
                let south = Pos::new(x, layer, z + 1);
                if rand.next_int_bound(4) == 0
                    && block_rules::material(world.block_state(west.x, west.y, west.z))
                        == block_rules::AIR
                {
                    swamp_vine(world, west, "east");
                }
                if rand.next_int_bound(4) == 0
                    && block_rules::material(world.block_state(east.x, east.y, east.z))
                        == block_rules::AIR
                {
                    swamp_vine(world, east, "west");
                }
                if rand.next_int_bound(4) == 0
                    && block_rules::material(world.block_state(north.x, north.y, north.z))
                        == block_rules::AIR
                {
                    swamp_vine(world, north, "south");
                }
                if rand.next_int_bound(4) == 0
                    && block_rules::material(world.block_state(south.x, south.y, south.z))
                        == block_rules::AIR
                {
                    swamp_vine(world, south, "north");
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenSwamp::addVine`（自带向下延伸）。
fn swamp_vine(world: &mut GenWorld, pos: Pos, face: &str) {
    let state = block_rules::vine_with_face(face);
    world.set_block_state(pos.x, pos.y, pos.z, state);
    let mut below = pos.down();
    let mut remaining = 4;
    while block_rules::material(world.block_state(below.x, below.y, below.z)) == block_rules::AIR
        && remaining > 0
    {
        world.set_block_state(below.x, below.y, below.z, state);
        below = below.down();
        remaining -= 1;
    }
}

// ---------------------------------------------------------------------------
// WorldGenCanopyTree：深色橡木（2x2 树干 + 平顶树冠）。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenCanopyTree::generate`。
fn generate_canopy(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let height = rand.next_int_bound(3) + rand.next_int_bound(2) + 6;
    let x = pos.x;
    let y = pos.y;
    let z = pos.z;
    if !(y >= 1 && y + height + 1 < HEIGHT) {
        return false;
    }

    let below = pos.down();
    let ground = world.block_state(below.x, below.y, below.z) >> 4;
    if !matches!(ground, 2 | 3 | 243) {
        return false;
    }
    if !canopy_space_at(world, pos, height) {
        return false;
    }

    set_dirt_at(world, below);
    set_dirt_at(world, below.east());
    set_dirt_at(world, below.south());
    set_dirt_at(world, below.south().east());

    let (dir_x, dir_z) = HORIZONTAL[rand.next_int_bound(4) as usize];
    let trunk_height = height - rand.next_int_bound(4);
    let mut bend = 2 - rand.next_int_bound(3);
    let mut trunk_x = x;
    let mut trunk_z = z;
    let crown_y = y + height - 1;

    for step in 0..height {
        if step >= trunk_height && bend > 0 {
            trunk_x += dir_x;
            trunk_z += dir_z;
            bend -= 1;
        }
        let layer = y + step;
        let log = Pos::new(trunk_x, layer, trunk_z);
        if matches!(
            block_rules::material(world.block_state(log.x, log.y, log.z)),
            block_rules::AIR | block_rules::LEAVES
        ) {
            canopy_log_at(world, log);
            canopy_log_at(world, log.east());
            canopy_log_at(world, log.south());
            canopy_log_at(world, log.east().south());
        }
    }

    for dx in -2i32..=0 {
        for dz in -2i32..=0 {
            canopy_leaf_at(world, trunk_x + dx, crown_y - 1, trunk_z + dz);
            canopy_leaf_at(world, (1 + trunk_x) - dx, crown_y - 1, trunk_z + dz);
            canopy_leaf_at(world, trunk_x + dx, crown_y - 1, (1 + trunk_z) - dz);
            canopy_leaf_at(world, (1 + trunk_x) - dx, crown_y - 1, (1 + trunk_z) - dz);
            if (dx > -2 || dz > -1) && (dx != -1 || dz != -2) {
                canopy_leaf_at(world, trunk_x + dx, crown_y + 1, trunk_z + dz);
                canopy_leaf_at(world, (1 + trunk_x) - dx, crown_y + 1, trunk_z + dz);
                canopy_leaf_at(world, trunk_x + dx, crown_y + 1, (1 + trunk_z) - dz);
                canopy_leaf_at(world, (1 + trunk_x) - dx, crown_y + 1, (1 + trunk_z) - dz);
            }
        }
    }

    if rand.next_boolean() {
        canopy_leaf_at(world, trunk_x, crown_y + 2, trunk_z);
        canopy_leaf_at(world, trunk_x + 1, crown_y + 2, trunk_z);
        canopy_leaf_at(world, trunk_x + 1, crown_y + 2, trunk_z + 1);
        canopy_leaf_at(world, trunk_x, crown_y + 2, trunk_z + 1);
    }

    for dx in -3i32..=4 {
        for dz in -3i32..=4 {
            if (dx != -3 || dz != -3)
                && (dx != -3 || dz != 4)
                && (dx != 4 || dz != -3)
                && (dx != 4 || dz != 4)
                && (dx.abs() < 3 || dz.abs() < 3)
            {
                canopy_leaf_at(world, trunk_x + dx, crown_y, trunk_z + dz);
            }
        }
    }

    for dx in -1..=2 {
        for dz in -1..=2 {
            if (dx < 0 || dx > 1 || dz < 0 || dz > 1) && rand.next_int_bound(3) <= 0 {
                let branch_len = rand.next_int_bound(3) + 2;
                for step in 0..branch_len {
                    canopy_log_at(world, Pos::new(x + dx, (crown_y - step) - 1, z + dz));
                }
                for ox in -1..=1 {
                    for oz in -1..=1 {
                        canopy_leaf_at(world, (trunk_x + dx) + ox, crown_y, (trunk_z + dz) + oz);
                    }
                }
                for ox in -2i32..=2 {
                    for oz in -2i32..=2 {
                        if ox.abs() != 2 || oz.abs() != 2 {
                            canopy_leaf_at(
                                world,
                                (trunk_x + dx) + ox,
                                crown_y - 1,
                                (trunk_z + dz) + oz,
                            );
                        }
                    }
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenCanopyTree::placeTreeOfHeight`。
fn canopy_space_at(world: &GenWorld, pos: Pos, height: i32) -> bool {
    for layer in 0..=height + 1 {
        let radius = if layer == 0 {
            0
        } else if layer >= height - 1 {
            2
        } else {
            1
        };
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if !can_grow_into(world.block_state(pos.x + dx, pos.y + layer, pos.z + dz) >> 4) {
                    return false;
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenCanopyTree::placeLogAt`。
fn canopy_log_at(world: &mut GenWorld, pos: Pos) {
    if can_grow_into(world.block_state(pos.x, pos.y, pos.z) >> 4) {
        world.set_block_state(pos.x, pos.y, pos.z, DARK_OAK_LOG);
    }
}

/// 对应 `WorldGenCanopyTree::placeLeafAt`。
fn canopy_leaf_at(world: &mut GenWorld, x: i32, y: i32, z: i32) {
    if block_rules::material(world.block_state(x, y, z)) == block_rules::AIR {
        world.set_block_state(x, y, z, DARK_OAK_LEAVES);
    }
}

// ---------------------------------------------------------------------------
// WorldGenShrub：丛林灌木。
// ---------------------------------------------------------------------------

/// 对应 `new WorldGenShrub(jungleLog, woodState(0, true))`。
fn generate_jungle_shrub(world: &mut GenWorld, rand: &mut JavaRandom, mut pos: Pos) -> bool {
    // 先把位置沉到地面（空气/树叶且 y > 0 就继续往下）。
    while matches!(
        block_rules::material(world.block_state(pos.x, pos.y, pos.z)),
        block_rules::AIR | block_rules::LEAVES
    ) && pos.y > 0
    {
        pos = pos.down();
    }
    if !matches!(world.block_state(pos.x, pos.y, pos.z) >> 4, 3 | 2) {
        return true;
    }

    pos = pos.up();
    world.set_block_state(pos.x, pos.y, pos.z, JUNGLE_LOG);
    for layer in pos.y..=pos.y + 2 {
        let from_top = layer - pos.y;
        let radius = 2 - from_top;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius || dz.abs() != radius || rand.next_int_bound(2) != 0 {
                    if matches!(
                        block_rules::material(world.block_state(x, layer, z)),
                        block_rules::AIR | block_rules::LEAVES
                    ) {
                        world.set_block_state(x, layer, z, OAK_LEAVES_DECAY);
                    }
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// WorldGenHugeTrees 公共部分 + WorldGenMegaPineTree / WorldGenMegaJungle。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenHugeTrees::getHeight`。
fn huge_tree_height(rand: &mut JavaRandom, base_height: i32, extra_random_height: i32) -> i32 {
    let mut height = rand.next_int_bound(3) + base_height;
    if extra_random_height > 1 {
        height += rand.next_int_bound(extra_random_height);
    }
    height
}

/// 对应 `WorldGenHugeTrees::isSpaceAt`。
fn huge_is_space_at(world: &GenWorld, pos: Pos, height: i32) -> bool {
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }
    for layer in 0..=1 + height {
        let radius = if layer == 0 { 1 } else { 2 };
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let y = pos.y + layer;
                if y < 0
                    || y >= HEIGHT
                    || !can_grow_into(world.block_state(pos.x + dx, y, pos.z + dz) >> 4)
                {
                    return false;
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenHugeTrees::ensureDirtsUnderneath`。
fn huge_ensure_dirts_underneath(world: &mut GenWorld, pos: Pos) -> bool {
    let below = pos.down();
    let ground = world.block_state(below.x, below.y, below.z) >> 4;
    if !matches!(ground, 2 | 3 | 243) || pos.y < 2 {
        return false;
    }
    set_dirt_at(world, below);
    set_dirt_at(world, below.east());
    set_dirt_at(world, below.south());
    set_dirt_at(world, below.south().east());
    true
}

/// 对应 `WorldGenHugeTrees::growLeavesLayerStrict`。
fn grow_leaves_layer_strict(world: &mut GenWorld, center: Pos, width: i32, leaves: i32) {
    let limit = width * width;
    for dx in -width..=width + 1 {
        for dz in -width..=width + 1 {
            let ox = dx - 1;
            let oz = dz - 1;
            if dx * dx + dz * dz <= limit
                || ox * ox + oz * oz <= limit
                || dx * dx + oz * oz <= limit
                || ox * ox + dz * dz <= limit
            {
                place_huge_leaves(world, center.add(dx, 0, dz), leaves);
            }
        }
    }
}

/// 对应 `WorldGenHugeTrees::growLeavesLayer`。
fn grow_leaves_layer(world: &mut GenWorld, center: Pos, width: i32, leaves: i32) {
    let limit = width * width;
    for dx in -width..=width {
        for dz in -width..=width {
            if dx * dx + dz * dz <= limit {
                place_huge_leaves(world, center.add(dx, 0, dz), leaves);
            }
        }
    }
}

fn place_huge_leaves(world: &mut GenWorld, pos: Pos, leaves: i32) {
    if matches!(
        block_rules::material(world.block_state(pos.x, pos.y, pos.z)),
        block_rules::AIR | block_rules::LEAVES
    ) {
        world.set_block_state(pos.x, pos.y, pos.z, leaves);
    }
}

/// 只在空气/树叶处放方块（巨型针叶树干用）。
fn place_if_growable(world: &mut GenWorld, pos: Pos, state: i32) {
    if matches!(
        block_rules::material(world.block_state(pos.x, pos.y, pos.z)),
        block_rules::AIR | block_rules::LEAVES
    ) {
        world.set_block_state(pos.x, pos.y, pos.z, state);
    }
}

/// 对应 `new WorldGenMegaPineTree(notify, useBaseHeight)`（baseHeight=13，
/// extraRandomHeight=15）。megaSpruce 只是 `useBaseHeight=true` 的变体。
fn generate_mega_pine(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    use_base_height: bool,
) -> bool {
    let height = huge_tree_height(rand, 13, 15);
    if !huge_is_space_at(world, pos, height) || !huge_ensure_dirts_underneath(world, pos) {
        return false;
    }

    // createCrown：自树顶向下铺 5 层左右的针叶。
    let crown = rand.next_int_bound(5) + if use_base_height { 13 } else { 3 };
    let mut previous = 0;
    for layer in (pos.y + height - crown)..=pos.y + height {
        let depth = (pos.y + height) - layer;
        let width = math_helper::floor(f(
            f(f(depth as f64) / f(crown as f64)) * 3.5,
        ));
        let extra = if depth > 0 && width == previous && (layer & 1) == 0 {
            1
        } else {
            0
        };
        grow_leaves_layer_strict(
            world,
            Pos::new(pos.x, layer, pos.z),
            width + extra,
            SPRUCE_LEAVES,
        );
        previous = width;
    }

    for step in 0..height {
        let trunk = pos.up_n(step);
        place_if_growable(world, trunk, SPRUCE_LOG);
        if step < height - 1 {
            place_if_growable(world, pos.add(1, step, 0), SPRUCE_LOG);
            place_if_growable(world, pos.add(1, step, 1), SPRUCE_LOG);
            place_if_growable(world, pos.add(0, step, 1), SPRUCE_LOG);
        }
    }
    true
}

/// 对应 `new WorldGenMegaJungle(false, 10, 20, jungleLog, jungleLeaves)`。
fn generate_mega_jungle(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let height = huge_tree_height(rand, 10, 20);
    if !huge_is_space_at(world, pos, height) || !huge_ensure_dirts_underneath(world, pos) {
        return false;
    }

    // createCrown(center, 2)
    let crown_center = pos.up_n(height);
    for layer in -2..=0 {
        grow_leaves_layer_strict(world, crown_center.up_n(layer), 3 - layer, JUNGLE_LEAVES);
    }

    // 侧枝：每段随机角度向外伸 5 格，末端挂一团树叶。
    let mut branch_y = (pos.y + height) - 2 - rand.next_int_bound(4);
    while branch_y > pos.y + height / 2 {
        let angle = f(rand.next_float() as f64 * f(f(std::f64::consts::PI) * 2.0));
        let mut bx = pos.x + f(0.5 + f(math_helper::cos(angle) * 4.0)) as i32;
        let mut bz = pos.z + f(0.5 + f(math_helper::sin(angle) * 4.0)) as i32;
        for step in 0..5 {
            bx = pos.x + f(1.5 + f(math_helper::cos(angle) * f(step as f64))) as i32;
            bz = pos.z + f(1.5 + f(math_helper::sin(angle) * f(step as f64))) as i32;
            world.set_block_state(bx, (branch_y - 3) + step / 2, bz, JUNGLE_LOG);
        }
        let spread = 1 + rand.next_int_bound(2);
        for layer in (branch_y - spread)..=branch_y {
            let from_top = layer - branch_y;
            grow_leaves_layer(world, Pos::new(bx, layer, bz), 1 - from_top, JUNGLE_LEAVES);
        }
        branch_y -= 2 + rand.next_int_bound(4);
    }

    // 2x2 主干 + 藤蔓（树干四列分别处理）。
    for step in 0..height {
        let trunk = pos.up_n(step);
        if can_grow_into(world.block_state(trunk.x, trunk.y, trunk.z) >> 4) {
            world.set_block_state(trunk.x, trunk.y, trunk.z, JUNGLE_LOG);
            if step > 0 {
                mega_jungle_vine(world, rand, trunk.west(), "east");
                mega_jungle_vine(world, rand, trunk.north(), "south");
            }
        }
        if step < height - 1 {
            let east = trunk.east();
            if can_grow_into(world.block_state(east.x, east.y, east.z) >> 4) {
                world.set_block_state(east.x, east.y, east.z, JUNGLE_LOG);
                if step > 0 {
                    mega_jungle_vine(world, rand, east.east(), "west");
                    mega_jungle_vine(world, rand, east.north(), "south");
                }
            }
            let south_east = east.south();
            if can_grow_into(world.block_state(south_east.x, south_east.y, south_east.z) >> 4) {
                world.set_block_state(south_east.x, south_east.y, south_east.z, JUNGLE_LOG);
                if step > 0 {
                    mega_jungle_vine(world, rand, south_east.east(), "west");
                    mega_jungle_vine(world, rand, south_east.south(), "north");
                }
            }
            let south = trunk.south();
            if can_grow_into(world.block_state(south.x, south.y, south.z) >> 4) {
                world.set_block_state(south.x, south.y, south.z, JUNGLE_LOG);
                if step > 0 {
                    mega_jungle_vine(world, rand, south.west(), "east");
                    mega_jungle_vine(world, rand, south.south(), "north");
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenMegaJungle::placeVine`：2/3 概率给空气格挂藤。
fn mega_jungle_vine(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, face: &str) {
    if rand.next_int_bound(3) > 0 && world.is_air(pos.x, pos.y, pos.z) {
        add_vine(world, pos, face);
    }
}

// ---------------------------------------------------------------------------
// WorldGenSavannaTree：金合欢（先斜长一段树干，再往上分岔）。
// ---------------------------------------------------------------------------

/// 对应 `WorldGenSavannaTree::generate`。
fn generate_savanna(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let height = rand.next_int_bound(3) + rand.next_int_bound(3) + 5;
    if !(pos.y >= 1 && pos.y + height + 1 <= HEIGHT) {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let radius = if y == pos.y {
            0
        } else if y >= pos.y + 1 + height - 2 {
            2
        } else {
            1
        };
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= HEIGHT || !can_grow_into(world.block_state(x, y, z) >> 4) {
                    valid = false;
                    break 'scan;
                }
            }
        }
    }
    if !valid {
        return false;
    }

    let ground = world.block_state(pos.x, pos.y - 1, pos.z) >> 4;
    if !matches!(ground, 2 | 3 | 243) || pos.y >= HEIGHT - height - 1 {
        return false;
    }
    set_dirt_at(world, pos.down());

    let dir = rand.next_int_bound(4) as usize;
    let (dir_x, dir_z) = HORIZONTAL[dir];
    let trunk_height = (height - rand.next_int_bound(4)) - 1;
    let mut bend = 3 - rand.next_int_bound(3);
    let mut trunk_x = pos.x;
    let mut trunk_z = pos.z;
    let mut last_y = 0;
    for step in 0..height {
        let layer = pos.y + step;
        if step >= trunk_height && bend > 0 {
            trunk_x += dir_x;
            trunk_z += dir_z;
            bend -= 1;
        }
        let log = Pos::new(trunk_x, layer, trunk_z);
        if matches!(
            block_rules::material(world.block_state(log.x, log.y, log.z)),
            block_rules::AIR | block_rules::LEAVES
        ) {
            world.set_block_state(log.x, log.y, log.z, ACACIA_LOG);
            last_y = layer;
        }
    }

    let mut crown = Pos::new(trunk_x, last_y, trunk_z);
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            if dx.abs() != 3 || dz.abs() != 3 {
                savanna_leaf_at(world, crown.add(dx, 0, dz));
            }
        }
    }
    crown = crown.up();
    for dx in -1..=1 {
        for dz in -1..=1 {
            savanna_leaf_at(world, crown.add(dx, 0, dz));
        }
    }
    savanna_leaf_at(world, crown.east());
    savanna_leaf_at(world, crown.east());
    savanna_leaf_at(world, crown.west());
    savanna_leaf_at(world, crown.west());
    savanna_leaf_at(world, crown.south());
    savanna_leaf_at(world, crown.south());
    savanna_leaf_at(world, crown.north());
    savanna_leaf_at(world, crown.north());

    // 第二根斜枝。
    trunk_x = pos.x;
    trunk_z = pos.z;
    let second = rand.next_int_bound(4) as usize;
    if second != dir {
        let (second_x, second_z) = HORIZONTAL[second];
        let mut branch_y = (trunk_height - rand.next_int_bound(2)) - 1;
        let mut branch_len = 1 + rand.next_int_bound(3);
        last_y = 0;
        while branch_y < height && branch_len > 0 {
            if branch_y >= 1 {
                let layer = pos.y + branch_y;
                trunk_x += second_x;
                trunk_z += second_z;
                let log = Pos::new(trunk_x, layer, trunk_z);
                if matches!(
                    block_rules::material(world.block_state(log.x, log.y, log.z)),
                    block_rules::AIR | block_rules::LEAVES
                ) {
                    world.set_block_state(log.x, log.y, log.z, ACACIA_LOG);
                    last_y = layer;
                }
            }
            branch_y += 1;
            branch_len -= 1;
        }
        if last_y > 0 {
            let top = Pos::new(trunk_x, last_y, trunk_z);
            for dx in -2i32..=2 {
                for dz in -2i32..=2 {
                    if dx.abs() != 2 || dz.abs() != 2 {
                        savanna_leaf_at(world, top.add(dx, 0, dz));
                    }
                }
            }
            let top = top.up();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    savanna_leaf_at(world, top.add(dx, 0, dz));
                }
            }
        }
    }
    true
}

/// 对应 `WorldGenSavannaTree::placeLeafAt`。
fn savanna_leaf_at(world: &mut GenWorld, pos: Pos) {
    if matches!(
        block_rules::material(world.block_state(pos.x, pos.y, pos.z)),
        block_rules::AIR | block_rules::LEAVES
    ) {
        world.set_block_state(pos.x, pos.y, pos.z, ACACIA_LEAVES);
    }
}

// ---------------------------------------------------------------------------
// 对外入口。
// ---------------------------------------------------------------------------

/// `VanillaBiomeDecorator::treeFor` 里可能返回的原版树。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VanillaTreeKind {
    Oak,
    BigOak,
    Birch,
    TallBirch,
    Canopy,
    Swamp,
    Taiga1,
    Taiga2,
    MegaPine,
    MegaSpruce,
    Jungle,
    JungleShrub,
    MegaJungle,
    Savanna,
}

/// 对应 `treeFor(...)->generate(world, rand, pos)`，返回是否成功生成。
///
/// 随机数消耗与 PHP 一致：`Jungle` 的 `4+rand.nextInt(7)` 在这里消耗
/// （PHP 是在 `treeFor` 里构造 `WorldGenTrees` 时消耗），`MegaJungle` 的
/// 高度由 `WorldGenHugeTrees::getHeight` 在 generate 内部决定。
pub fn generate_vanilla_named_tree(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    kind: VanillaTreeKind,
) -> bool {
    match kind {
        // new WorldGenTrees(false)：默认树高 4、不带藤蔓。
        VanillaTreeKind::Oak => generate_vanilla_tree(world, rand, pos, TreeMaterials::OAK),
        // new WorldGenBigTree(false)
        VanillaTreeKind::BigOak => generate_big_tree(world, rand, pos),
        // new WorldGenBirchTree(false, false)
        VanillaTreeKind::Birch => generate_birch(world, rand, pos, false),
        // new WorldGenBirchTree(false, true)
        VanillaTreeKind::TallBirch => generate_birch(world, rand, pos, true),
        // new WorldGenCanopyTree(false)
        VanillaTreeKind::Canopy => generate_canopy(world, rand, pos),
        // new WorldGenSwamp()
        VanillaTreeKind::Swamp => generate_swamp(world, rand, pos),
        // new WorldGenTaiga1()
        VanillaTreeKind::Taiga1 => generate_taiga1(world, rand, pos),
        // new WorldGenTaiga2(false)
        VanillaTreeKind::Taiga2 => generate_taiga2(world, rand, pos),
        // new WorldGenMegaPineTree(false, false)
        VanillaTreeKind::MegaPine => generate_mega_pine(world, rand, pos, false),
        // new WorldGenMegaPineTree(false, true)
        VanillaTreeKind::MegaSpruce => generate_mega_pine(world, rand, pos, true),
        // new WorldGenTrees(false, 4+rand.nextInt(7), woodState(3,false),
        //                  woodState(3,true), true)
        VanillaTreeKind::Jungle => {
            let height = 4 + rand.next_int_bound(7);
            generate_trees(world, rand, pos, height, JUNGLE_LOG, JUNGLE_LEAVES, true)
        }
        // new WorldGenShrub(jungleLog, woodState(0, true))
        VanillaTreeKind::JungleShrub => generate_jungle_shrub(world, rand, pos),
        // new WorldGenMegaJungle(false, 10, 20, jungleLog, jungleLeaves)
        VanillaTreeKind::MegaJungle => generate_mega_jungle(world, rand, pos),
        // new WorldGenSavannaTree(false)
        VanillaTreeKind::Savanna => generate_savanna(world, rand, pos),
    }
}
