//! 对应 `rtg/feature/{WorldGenTrees,WorldGenShrubRTG}.php`。
//!
//! `DecoVariableTree` 在树高不足时会退回这两种小型树，因此它们属于森林树链，
//! 不是独立的 vanilla 装饰入口。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::{Tree, TreeMaterials, TreeRTG, f};
use crate::level::generator::world::GenWorld;

/// 对应 `new WorldGenTrees(false, 4, log, leaves, false)`。
pub fn generate_vanilla_tree(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    materials: TreeMaterials,
) -> bool {
    let height = rand.next_int_bound(3) + 4;
    if pos.y < 1 || pos.y + height + 1 > 256 {
        return false;
    }

    let mut valid = true;
    'scan: for y in pos.y..=pos.y + 1 + height {
        let mut radius = 1;
        if y == pos.y {
            radius = 0;
        }
        if y >= pos.y + 1 + height - 2 {
            radius = 2;
        }
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                if y < 0 || y >= 256 || !can_grow_into(world.block_state(x, y, z) >> 4) {
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
    if !matches!(ground, 2 | 3 | 60 | 243) || pos.y >= 256 - height - 1 {
        return false;
    }
    // Java 的灰化土也是 dirt（id 3），种树时不应改成普通泥土
    if !matches!(ground, 3 | 243) {
        world.set_block_state(pos.x, pos.y - 1, pos.z, 48);
    }

    for y in pos.y - 3 + height..=pos.y + height {
        let from_top = y - (pos.y + height);
        let radius = 1 - from_top / 2;
        for x in pos.x - radius..=pos.x + radius {
            let dx = x - pos.x;
            for z in pos.z - radius..=pos.z + radius {
                let dz = z - pos.z;
                if dx.abs() != radius
                    || dz.abs() != radius
                    || (rand.next_int_bound(2) != 0 && from_top != 0)
                {
                    let state = world.block_state(x, y, z);
                    if matches!(
                        block_rules::material(state),
                        block_rules::AIR | block_rules::LEAVES | block_rules::VINE
                    ) {
                        world.set_block_state(x, y, z, materials.leaves);
                    }
                }
            }
        }
    }

    for step in 0..height {
        let target = pos.up_n(step);
        if matches!(
            block_rules::material(world.block_state(target.x, target.y, target.z)),
            block_rules::AIR | block_rules::LEAVES | block_rules::VINE
        ) {
            world.set_block_state(target.x, target.y, target.z, materials.log);
        }
    }

    // vinesGrow=false，藤蔓两段完全不执行；可可果的抽样仍然保留。
    if rand.next_int_bound(5) == 0 && height > 5 {
        for layer in 0..2 {
            // Facing::horizontal(): NORTH, EAST, SOUTH, WEST。
            for (dx, dz, facing) in [(0, -1, 2), (1, 0, 3), (0, 1, 0), (-1, 0, 1)] {
                if rand.next_int_bound(4 - layer) == 0 {
                    let age = rand.next_int_bound(3);
                    let target = pos.add(-dx, height - 5 + layer, -dz);
                    world.set_block_state(
                        target.x,
                        target.y,
                        target.z,
                        (127 << 4) | (age << 2) | facing,
                    );
                }
            }
        }
    }
    true
}

fn can_grow_into(block_id: i32) -> bool {
    matches!(
        block_rules::material(block_rules::default_state(block_id)),
        block_rules::AIR | block_rules::LEAVES
    ) || matches!(block_id, 2 | 3 | 17 | 162 | 6 | 106)
}

/// 对应 `WorldGenShrubRTG::generate`。
pub fn generate_shrub(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    size: i32,
    materials: TreeMaterials,
) -> bool {
    generate_shrub_with_sand(world, rand, pos, size, materials, false)
}

pub fn generate_shrub_with_sand(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    size: i32,
    materials: TreeMaterials,
    sand: bool,
) -> bool {
    let width = size.min(6);
    let height = if size > 3 { 2 } else { 1 };
    for i in 0..size {
        let rx = rand.next_int_bound(width * 2) - width;
        let ry = rand.next_int_bound(height);
        let rz = rand.next_int_bound(width * 2) - width;
        if i == 0 && size > 4 {
            build_shrub_leaves(world, pos.x + rx, pos.y, pos.z + rz, 3, materials, sand);
        } else if i == 1 && size > 2 {
            build_shrub_leaves(world, pos.x + rx, pos.y, pos.z + rz, 2, materials, sand);
        } else {
            build_shrub_leaves(
                world,
                pos.x + rx,
                pos.y + ry,
                pos.z + rz,
                1,
                materials,
                sand,
            );
        }
    }
    true
}

fn build_shrub_leaves(
    world: &mut GenWorld,
    x: i32,
    y: i32,
    z: i32,
    size: i32,
    materials: TreeMaterials,
    sand: bool,
) {
    let ground = world.block_state(x, y - 2, z);
    let above = world.block_state(x, y - 1, z);
    let ground_material = block_rules::material(ground);
    if !(matches!(ground_material, block_rules::GRASS | block_rules::GROUND)
        || (sand && ground_material == block_rules::SAND))
        || above == 144
    {
        return;
    }
    for dx in -size..=size {
        for dy in -1_i32..=1 {
            for dz in -size..=size {
                if dx.abs() + dy.abs() + dz.abs() <= size {
                    let (tx, ty, tz) = (x + dx, y + dy, z + dz);
                    let state = world.block_state(tx, ty, tz);
                    if state >> 4 != 83
                        && matches!(
                            block_rules::material(state),
                            block_rules::AIR | block_rules::VINE | block_rules::PLANTS
                        )
                    {
                        world.set_block_state(tx, ty, tz, materials.leaves);
                    }
                }
            }
        }
    }
    world.set_block_state(x, y - 1, z, materials.log);
}

/// 对应 `rtg/feature/WorldGenCacti.php`。
///
/// `soil_block` 是 state（`id<<4|meta`），默认 192 即沙子；`sand_only=true` 时
/// 只认沙子，否则草(32)/土(48)也可以。
pub fn generate_cacti(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    sand_only: bool,
    extra_height: i32,
    soil_block: i32,
) -> bool {
    let x = pos.x;
    let z = pos.z;
    let y = pos.y + rand.next_int_bound(4) - rand.next_int_bound(4);
    if !world.is_air(x, y, z) {
        return true;
    }
    let below = world.block_state(x, y - 1, z);
    let soil_ok = below == soil_block || (!sand_only && (below == 32 || below == 48));
    if !soil_ok {
        return true;
    }
    let inner = rand.next_int_bound(3) + 1;
    let height = 1 + rand.next_int_bound(inner);
    if below == 32 || below == 48 {
        world.set_block_state(x, y - 1, z, soil_block);
    }
    for dy in 0..height + extra_height {
        if block_rules::can_block_stay(81, world, Pos::new(x, y + dy, z)) {
            world.set_block_state(x, y + dy, z, 81 << 4);
        }
    }
    true
}

/// 对应 `rtg/feature/WorldGenBush.php`（蘑菇丛）。
///
/// `block` 是方块 id（39 棕蘑菇 / 40 红蘑菇）。
pub fn generate_bush(world: &mut GenWorld, rand: &mut JavaRandom, position: Pos, block: i32) -> bool {
    for _ in 0..64 {
        let blockpos = position.add(
            rand.next_int_bound(8) - rand.next_int_bound(8),
            rand.next_int_bound(4) - rand.next_int_bound(4),
            rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if world.is_air(blockpos.x, blockpos.y, blockpos.z)
            && block_rules::can_block_stay(block, world, blockpos)
        {
            world.set_block_state(
                blockpos.x,
                blockpos.y,
                blockpos.z,
                block_rules::default_state(block),
            );
        }
    }
    true
}

/// 对应 `rtg/feature/WorldGenBlob.php`（巨石）。
///
/// `blob_block` 是 state（如 64 = cobblestone），`valid_ground` 是 id 列表。
pub fn generate_blob(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    blob_block: i32,
    blob_size: i32,
    valid_ground: &[i32],
    allow_in_water: bool,
) -> bool {
    let mut x = pos.x;
    let mut y = pos.y;
    let mut z = pos.z;
    let mut mpos = pos;

    loop {
        mpos = mpos.down();
        let mut ground_found = false;
        if mpos.y > 3 {
            if !world.is_air(mpos.x, mpos.y, mpos.z) {
                let blockstate = world.block_state(mpos.x, mpos.y, mpos.z);
                if !allow_in_water
                    && (block_rules::material(blockstate) == block_rules::WATER
                        || block_rules::adjacent_water(world, mpos))
                {
                    return false;
                }
                if valid_ground.contains(&(blockstate >> 4)) {
                    ground_found = true;
                }
            }
            if !ground_found {
                y -= 1;
                continue;
            }
        }
        if mpos.y <= 3 {
            return false;
        }

        let k2 = blob_size;
        for _ in 0..3 {
            let size_x = blob_size + rand.next_int_bound(2);
            let size_y = blob_size + rand.next_int_bound(2);
            let size_z = blob_size + rand.next_int_bound(2);
            let f = ((size_x + size_y + size_z) as f64 * 0.333_000_004_291_534_4 + 0.5) as f32;
            let f_sq = (f * f) as f64;
            for bx in (x - size_x)..=(x + size_x) {
                for bz in (z - size_z)..=(z + size_z) {
                    for by in (y - size_y)..=(y + size_y) {
                        let dx = (bx - x) as f64;
                        let dz = (bz - z) as f64;
                        let dy = (by - y) as f64;
                        if dx * dx + dz * dz + dy * dy <= f_sq {
                            place_boulder_block(world, Pos::new(bx, by, bz), blob_block);
                        }
                    }
                }
            }
            x += -(blob_size + 1) + rand.next_int_bound(2 + k2 * 2);
            z += -(blob_size + 1) + rand.next_int_bound(2 + k2 * 2);
            y -= rand.next_int_bound(2);
        }
        return true;
    }
}

fn place_boulder_block(world: &mut GenWorld, target: Pos, boulder_block: i32) {
    if block_rules::is_replaceable(world, target) {
        world.set_block_state(target.x, target.y, target.z, boulder_block);
        let up = target.up();
        if world.block_state(up.x, up.y, up.z) >> 4 == 175 {
            world.set_block_state(up.x, up.y, up.z, 0);
        }
    }
}

/// 对应 `rtg/feature/WorldGenSponge.php`（海绵团）。
///
/// 和巨石类似的椭球团，但只在「六邻中至少 `min_adjacents` 个是水(9)或海绵(19)」
/// 的格子上放置。`sponge_block` 是 state（如 305 = 湿海绵）。
pub fn generate_sponge(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    sponge_block: i32,
    sponge_size: i32,
    valid_ground: &[i32],
) -> bool {
    let mut x = pos.x;
    let mut y = pos.y;
    let mut z = pos.z;

    // 往下走到第一个有效地基
    loop {
        if y > 3 {
            let below = world.block_state(x, y - 1, z);
            if !world.is_air(x, y - 1, z) && valid_ground.contains(&(below >> 4)) {
                break;
            }
            y -= 1;
            continue;
        }
        if y <= 3 {
            return false;
        }
    }

    let k2 = sponge_size;
    for _ in 0..3 {
        if k2 < 0 {
            break;
        }
        let i1 = k2 + rand.next_int_bound(2);
        let j1 = k2 + rand.next_int_bound(2);
        let k1 = k2 + rand.next_int_bound(2);
        let f = f(f((i1 + j1 + k1) as f64 * 0.333_000_004_291_534_4) + 0.5);
        let f_sq = (f * f) as f64;
        for l1 in (x - i1)..=(x + i1) {
            for i2 in (z - k1)..=(z + k1) {
                for j2 in (y - j1)..=(y + j1) {
                    let dx = (l1 - x) as f64;
                    let dz = (i2 - z) as f64;
                    let dy = (j2 - y) as f64;
                    if dx * dx + dz * dz + dy * dy <= f_sq
                        && sponge_has_valid_adjacent(world, l1, j2, i2)
                    {
                        world.set_block_state(l1, j2, i2, sponge_block);
                    }
                }
            }
        }
        x += -(k2 + 1) + rand.next_int_bound(2 + k2 * 2);
        z += -(k2 + 1) + rand.next_int_bound(2 + k2 * 2);
        y -= rand.next_int_bound(2);
    }
    true
}

/// 六邻中海绵(19)/水(9) 的数量 >= 3。
fn sponge_has_valid_adjacent(world: &GenWorld, x: i32, y: i32, z: i32) -> bool {
    let mut count = 0;
    for (dx, dy, dz) in [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ] {
        let id = world.block_state(x + dx, y + dy, z + dz) >> 4;
        if id == 19 || id == 9 {
            count += 1;
        }
    }
    count >= 3
}

/// 对应 `rtg/feature/WorldGenPumpkin.php`。
///
/// 南瓜只长在草方块上，朝向随机。
pub fn generate_pumpkin(world: &mut GenWorld, rand: &mut JavaRandom, position: Pos) -> bool {
    for _ in 0..64 {
        let blockpos = position.add(
            rand.next_int_bound(8) - rand.next_int_bound(8),
            rand.next_int_bound(4) - rand.next_int_bound(4),
            rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        let below_id = world.block_state(blockpos.x, blockpos.y - 1, blockpos.z) >> 4;
        if world.is_air(blockpos.x, blockpos.y, blockpos.z)
            && below_id == 2
            && block_rules::can_place_block_at(86, world, blockpos)
        {
            let facing = rand.next_int_bound(4);
            world.set_block_state(blockpos.x, blockpos.y, blockpos.z, (86 << 4) | facing);
        }
    }
    true
}

/// 对应 `rtg/feature/WorldGenReed.php`。
///
/// 在 position 周围随机找空气格，若其下方方块的四邻有水则往上叠甘蔗。
pub fn generate_reed(world: &mut GenWorld, rand: &mut JavaRandom, position: Pos) -> bool {
    for _ in 0..20 {
        let blockpos = position.add(
            rand.next_int_bound(4) - rand.next_int_bound(4),
            0,
            rand.next_int_bound(4) - rand.next_int_bound(4),
        );
        if !world.is_air(blockpos.x, blockpos.y, blockpos.z) {
            continue;
        }
        let below = blockpos.down();
        let has_water = [below.west(), below.east(), below.north(), below.south()]
            .iter()
            .any(|side| {
                block_rules::material(world.block_state(side.x, side.y, side.z))
                    == block_rules::WATER
            });
        if !has_water {
            continue;
        }
        let inner = rand.next_int_bound(3) + 1;
        let height = 2 + rand.next_int_bound(inner);
        for k in 0..height {
            let target = blockpos.up_n(k);
            if block_rules::can_block_stay(83, world, target) {
                world.set_block_state(target.x, target.y, target.z, 83 << 4);
            }
        }
    }
    true
}

/// 对应 `rtg/feature/WorldGenDeadBush.php`。
///
/// 从传入位置往下走到第一个非空气/非树叶方块，然后在周围随机撒 4 丛枯草。
pub fn generate_dead_bush(world: &mut GenWorld, rand: &mut JavaRandom, mut pos: Pos) -> bool {
    while (matches!(
        block_rules::material(world.block_state(pos.x, pos.y, pos.z)),
        block_rules::AIR | block_rules::LEAVES
    )) && pos.y > 0
    {
        pos = pos.down();
    }
    for _ in 0..4 {
        let target = pos.add(
            rand.next_int_bound(8) - rand.next_int_bound(8),
            rand.next_int_bound(4) - rand.next_int_bound(4),
            rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if world.is_air(target.x, target.y, target.z)
            && block_rules::can_block_stay(32, world, target)
        {
            world.set_block_state(target.x, target.y, target.z, 32 << 4);
        }
    }
    true
}

/// 对应 `rtg/feature/WorldGenCrops.php`（农作物斑块）。
///
/// `crop_type`：0=胡萝卜(141)、1=土豆(142)、2=甜菜根(207)、其他=小麦(59)。
/// 在泥土/草地上随机撒 `density` 株作物，脚下换成耕地(60)。
/// `water=true` 时在中心放一格水源。
pub fn generate_crops(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    crop_type: i32,
    size: i32,
    density: i32,
    height: i32,
    water: bool,
) -> bool {
    let crop_block = match crop_type {
        0 => 141,
        1 => 142,
        2 => 207,
        _ => 59,
    };
    let max_growth = if crop_block == 207 { 4 } else { 8 };

    let mut y = pos.y;
    // 往下走到第一个非空气/非树叶方块
    while y > 0 {
        let state = world.block_state(pos.x, y, pos.z);
        if !world.is_air(pos.x, y, pos.z) && !matches!(state >> 4, 18 | 161) {
            break;
        }
        y -= 1;
    }
    let ground = world.block_state(pos.x, y, pos.z) >> 4;
    if !matches!(ground, 2 | 3) {
        return false;
    }
    // 四邻必须是泥土或草
    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        let m = block_rules::material(world.block_state(pos.x + dx, y, pos.z + dz));
        if m != block_rules::GROUND && m != block_rules::GRASS {
            return false;
        }
    }

    for _ in 0..density {
        let rx = rand.next_int_bound(size) - 2;
        let ry = rand.next_int_bound(height) - 1;
        let rz = rand.next_int_bound(size) - 2;
        let gx = pos.x + rx;
        let gy = y + ry;
        let gz = pos.z + rz;
        let below = world.block_state(gx, gy, gz) >> 4;
        if matches!(below, 2 | 3) && world.is_air(gx, gy + 1, gz) {
            world.set_block_state(gx, gy, gz, (60 << 4) | rand.next_int_bound(8));
            world.set_block_state(gx, gy + 1, gz, (crop_block << 4) | rand.next_int_bound(max_growth));
        }
    }
    if water {
        world.set_block_state(pos.x, y, pos.z, 9 << 4);
    }
    true
}

/// 对应 `rtg/feature/WorldGenMinable.php`（矿石团）。
///
/// 生成一个弯曲的椭球状矿脉，只替换「天然石头」（state 16/17/19/21，即石头、
/// 花岗岩、闪长岩、安山岩）。所有浮点运算都按 f32 截断以匹配 PHP。
pub fn generate_minable(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    ore_block: i32,
    number_of_blocks: i32,
) -> bool {
    let pi = f(3.141592653589793);
    let angle = f(rand.next_float() as f64 * pi);
    let nb = f(number_of_blocks as f64);
    let sin_a = f(angle.sin());
    let cos_a = f(angle.cos());

    let x8 = f((pos.x + 8) as f64);
    let z8 = f((pos.z + 8) as f64);
    let sin_len = f(f(sin_a * nb) / f(8.0));
    let cos_len = f(f(cos_a * nb) / f(8.0));

    let d0 = f(x8 + sin_len);
    let d1 = f(x8 - sin_len);
    let d2 = f(z8 + cos_len);
    let d3 = f(z8 - cos_len);
    let d4 = f((pos.y + rand.next_int_bound(3) - 2) as f64);
    let d5 = f((pos.y + rand.next_int_bound(3) - 2) as f64);

    for i in 0..number_of_blocks {
        let f1 = f(i as f64 / nb);
        let d6 = f(d0 + f(f(d1 - d0) * f1));
        let d7 = f(d4 + f(f(d5 - d4) * f1));
        let d8 = f(d2 + f(f(d3 - d2) * f1));

        let d9 = f(f(rand.next_double() * nb) / f(16.0));
        let sin_curve = f(f(f(pi * f1)).sin() + f(1.0));
        let d10 = f(f(sin_curve * d9) + f(1.0));
        let d11 = d10;

        let j = f(d6 - f(d10 / f(2.0))).floor() as i32;
        let k = f(d7 - f(d11 / f(2.0))).floor() as i32;
        let l = f(d8 - f(d10 / f(2.0))).floor() as i32;
        let i1 = f(d6 + f(d10 / f(2.0))).floor() as i32;
        let j1 = f(d7 + f(d11 / f(2.0))).floor() as i32;
        let k1 = f(d8 + f(d10 / f(2.0))).floor() as i32;

        for lx in j..=i1 {
            let dx = f(f(f(lx as f64 + f(0.5)) - d6) / f(d10 / f(2.0)));
            let dx2 = f(dx * dx);
            if dx2 < f(1.0) {
                for ly in k..=j1 {
                    let dy = f(f(f(ly as f64 + f(0.5)) - d7) / f(d11 / f(2.0)));
                    let dxy = f(dx2 + f(dy * dy));
                    if dxy < f(1.0) {
                        for lz in l..=k1 {
                            let dz = f(f(f(lz as f64 + f(0.5)) - d8) / f(d10 / f(2.0)));
                            let dxyz = f(dxy + f(dz * dz));
                            if dxyz < f(1.0) {
                                let state = world.block_state(lx, ly, lz);
                                if is_natural_stone(state) {
                                    world.set_block_state(lx, ly, lz, ore_block);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    true
}

/// 对应 `NaturalStone::test`：只接受石头(16)、花岗岩(17)、闪长岩(19)、安山岩(21)。
fn is_natural_stone(state: i32) -> bool {
    matches!(state, 16 | 17 | 19 | 21)
}

/// 对应 `new WorldGenTrees(false)`：最小号橡树。
///
/// Mesa 高原等 biome 的 `DecoTree` 用 `treeType=1` 走 vanilla 路径时需要一个
/// 实现 `Tree` trait 的包装。
#[derive(Clone, Debug)]
pub struct VanillaOakTree {
    pub base: TreeRTG,
}

impl Default for VanillaOakTree {
    fn default() -> Self {
        Self {
            base: TreeRTG {
                log_block: block_rules::wood_state(0, false),
                leaves_block: block_rules::wood_state(0, true),
                ..TreeRTG::default()
            },
        }
    }
}

impl Tree for VanillaOakTree {
    fn furthest_likely_extension(&self) -> i32 {
        3
    }

    fn estimated_size(&self) -> f64 {
        f(f(3.0 * 3.0) / 32.0)
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        generate_vanilla_tree(world, rand, pos, TreeMaterials::OAK)
    }
}

// ---------------------------------------------------------------------------
// WorldGen 抽象：对应 `rtg/feature/WorldGenerator.php`。
// `DecoWorldGen` 持有一个 `Box<dyn WorldGen>`，在地表高度处调用 generate。
// ---------------------------------------------------------------------------

/// 对应 `rtg/feature/WorldGenerator.php`。
pub trait WorldGen {
    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, position: Pos) -> bool;
}

/// 对应 `rtg/feature/WorldGenBigMushroom.php`（巨型蘑菇）。
///
/// `mushroom_type`：`Some(99)` 棕色、`Some(100)` 红色、`None` 随机。
pub struct WorldGenBigMushroom {
    pub mushroom_type: Option<i32>,
}

impl Default for WorldGenBigMushroom {
    fn default() -> Self {
        Self { mushroom_type: None }
    }
}

impl WorldGen for WorldGenBigMushroom {
    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, position: Pos) -> bool {
        let block = match self.mushroom_type {
            Some(t) => t,
            None => {
                if rand.next_boolean() {
                    99
                } else {
                    100
                }
            }
        };

        let mut height = rand.next_int_bound(3) + 4;
        if rand.next_int_bound(12) == 0 {
            height *= 2;
        }

        let pos_y = position.y;
        // 空间检查：整根蘑菇占用的立方体内只能是空气或树叶。
        if !(pos_y >= 1 && pos_y + height + 1 < 256) {
            return false;
        }
        let mut flag = true;
        'scan: for j in pos_y..=pos_y + 1 + height {
            let k = if j <= pos_y + 3 { 0 } else { 3 };
            for l in (position.x - k)..=(position.x + k) {
                for i1 in (position.z - k)..=(position.z + k) {
                    if j >= 0 && j < 256 {
                        let material = block_rules::material(world.block_state(l, j, i1));
                        if material != block_rules::AIR && material != block_rules::LEAVES {
                            flag = false;
                            break 'scan;
                        }
                    } else {
                        flag = false;
                        break 'scan;
                    }
                }
            }
        }
        if !flag {
            return false;
        }

        // 地基：草(2) / 土(3) / 菌丝体(110)。
        let ground = world.block_state(position.x, pos_y - 1, position.z) >> 4;
        if !matches!(ground, 2 | 3 | 110) {
            return false;
        }

        // 蘑菇盖顶部起始高度。红蘑菇比棕蘑菇矮一截。
        let cap_top = pos_y + height;
        let cap_bottom = if block == 100 {
            pos_y + height - 3
        } else {
            cap_top
        };

        for l2 in cap_bottom..=cap_top {
            let mut j3 = 1;
            if l2 < cap_top {
                j3 += 1;
            }
            if block == 99 {
                j3 = 3;
            }
            let k3 = position.x - j3;
            let l3 = position.x + j3;
            let j1 = position.z - j3;
            let k1 = position.z + j3;

            for l1 in k3..=l3 {
                for i2 in j1..=k1 {
                    let mut j2: i32 = 5;
                    if l1 == k3 {
                        j2 -= 1;
                    } else if l1 == l3 {
                        j2 += 1;
                    }
                    if i2 == j1 {
                        j2 -= 3;
                    } else if i2 == k1 {
                        j2 += 3;
                    }
                    let mut variant = j2;

                    if block == 99 || l2 < cap_top {
                        // 四角不放置
                        if (l1 == k3 || l1 == l3) && (i2 == j1 || i2 == k1) {
                            continue;
                        }
                        // 内圈边沿的 variant 修正
                        if l1 == position.x - (j3 - 1) && i2 == j1 {
                            variant = 1;
                        }
                        if l1 == k3 && i2 == position.z - (j3 - 1) {
                            variant = 1;
                        }
                        if l1 == position.x + (j3 - 1) && i2 == j1 {
                            variant = 3;
                        }
                        if l1 == l3 && i2 == position.z - (j3 - 1) {
                            variant = 3;
                        }
                        if l1 == position.x - (j3 - 1) && i2 == k1 {
                            variant = 7;
                        }
                        if l1 == k3 && i2 == position.z + (j3 - 1) {
                            variant = 7;
                        }
                        if l1 == position.x + (j3 - 1) && i2 == k1 {
                            variant = 9;
                        }
                        if l1 == l3 && i2 == position.z + (j3 - 1) {
                            variant = 9;
                        }
                    }

                    // 盖的非顶部层，中心 variant 由 5 改为 0
                    if variant == 5 && l2 < cap_top {
                        variant = 0;
                    }

                    // 顶部层以下且 variant==0 的格子不放
                    if pos_y >= cap_top - 1 || variant != 0 {
                        let target = Pos::new(l1, l2, i2);
                        if !block_rules::is_full_block(world.block_state(l1, l2, i2)) {
                            world.set_block_state(
                                target.x,
                                target.y,
                                target.z,
                                (block << 4) | variant,
                            );
                        }
                    }
                }
            }
        }

        // 蘑菇柄：variant 10
        for i3 in 0..height {
            let target = position.up_n(i3);
            if !block_rules::is_full_block(
                world.block_state(target.x, target.y, target.z),
            ) {
                world.set_block_state(target.x, target.y, target.z, (block << 4) | 10);
            }
        }
        true
    }
}

#[cfg(test)]
mod big_mushroom_tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;
    use crate::level::generator::realistic_terrain::math::JavaRandom;

    fn mycelium_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 110 << 4); // 菌丝体
            }
        }
        world
    }

    #[test]
    fn vanilla_tree_keeps_native_podzol_ground() {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 243 << 4); // 落地后的灰化土
            }
        }
        let mut rand = JavaRandom::new(1);
        let ok = generate_vanilla_tree(
            &mut world,
            &mut rand,
            Pos::new(8, 64, 8),
            TreeMaterials::OAK,
        );
        assert!(ok, "灰化土上应能生成橡树");
        assert_eq!(world.block_state(8, 63, 8), 243 << 4, "灰化土不应被改成泥土");
    }

    #[test]
    fn big_mushroom_grows_on_mycelium() {
        let mut world = mycelium_world();
        let mut rand = JavaRandom::new(42);
        let mut shroom = WorldGenBigMushroom::default();
        let ok = shroom.generate(&mut world, &mut rand, Pos::new(8, 64, 8));
        assert!(ok, "菌丝体上应能生成大蘑菇");

        // 蘑菇柄应存在（variant 10）
        let stem = (0..16).any(|y| {
            let s = world.block_state(8, 64 + y, 8);
            (s >> 4 == 99 || s >> 4 == 100) && (s & 15) == 10
        });
        assert!(stem, "应存在蘑菇柄（variant 10）");
    }
}
