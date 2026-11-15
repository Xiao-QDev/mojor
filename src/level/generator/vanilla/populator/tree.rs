// 对齐 PopulatorTree 与 object/tree 包：橡树 白桦 高白桦 云杉 丛林树
use crate::level::format::chunk::CHUNK_HEIGHT;
use crate::level::generator::blocks::*;
use crate::level::generator::populator::random_range;
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

pub fn populate(
    world: &mut GenWorld,
    chunk_x: i32,
    chunk_z: i32,
    random: &mut NukkitRandom,
    sapling: u8,
    base_amount: i32,
    random_amount: i32,
) {
    let amount = random.next_bounded_int(random_amount + 1) + base_amount;
    for _ in 0..amount {
        let x = random_range(random, chunk_x << 4, (chunk_x << 4) + 15);
        let z = random_range(random, chunk_z << 4, (chunk_z << 4) + 15);
        let y = highest_workable_block(world, x, z);
        if y == -1 {
            continue;
        }
        grow_tree(world, random, x, y, z, sapling);
    }
}

// 对齐 PopulatorTree.getHighestWorkableBlock
fn highest_workable_block(world: &GenWorld, x: i32, z: i32) -> i32 {
    let mut y = CHUNK_HEIGHT as i32 - 1;
    while y > 0 {
        let id = world.block(x, y, z);
        if id == DIRT || id == GRASS {
            break;
        }
        if id != AIR && id != SNOW_LAYER {
            return -1;
        }
        y -= 1;
    }
    y + 1
}

fn grow_tree(world: &mut GenWorld, random: &mut NukkitRandom, x: i32, y: i32, z: i32, sapling: u8) {
    // 树高在 canPlaceObject 里用的是默认值 放置时才重新随机
    let (check_height, kind) = match sapling {
        SAPLING_SPRUCE => {
            let _ = random.next_bounded_int(39);
            (15, TreeKind::Spruce)
        }
        SAPLING_BIRCH => {
            if random.next_bounded_int(39) == 0 {
                (7, TreeKind::TallBirch)
            } else {
                (7, TreeKind::Birch)
            }
        }
        SAPLING_JUNGLE => (8, TreeKind::Jungle),
        _ => (7, TreeKind::Oak),
    };

    if !can_place_object(world, x, y, z, check_height) {
        return;
    }
    place_object(world, random, x, y, z, kind);
}

#[derive(Clone, Copy)]
enum TreeKind {
    Oak,
    Birch,
    TallBirch,
    Spruce,
    Jungle,
}

fn can_place_object(world: &GenWorld, x: i32, y: i32, z: i32, tree_height: i32) -> bool {
    let mut radius = 0;
    for yy in 0..tree_height + 3 {
        if yy == 1 || yy == tree_height {
            radius += 1;
        }
        for xx in -radius..=radius {
            for zz in -radius..=radius {
                if !is_overridable(world.block(x + xx, y + yy, z + zz)) {
                    return false;
                }
            }
        }
    }
    true
}

fn place_object(
    world: &mut GenWorld,
    random: &mut NukkitRandom,
    x: i32,
    y: i32,
    z: i32,
    kind: TreeKind,
) {
    let (tree_height, log, meta) = match kind {
        TreeKind::Oak => (random.next_bounded_int(3) + 4, LOG, WOOD_OAK),
        TreeKind::Birch => (random.next_bounded_int(3) + 5, LOG, WOOD_BIRCH),
        TreeKind::TallBirch => (random.next_bounded_int(3) + 10, LOG, WOOD_BIRCH),
        TreeKind::Jungle => (random.next_bounded_int(6) + 4, LOG, WOOD_JUNGLE),
        TreeKind::Spruce => (random.next_bounded_int(10) + 5, LOG, WOOD_SPRUCE),
    };

    if matches!(kind, TreeKind::Spruce) {
        place_spruce(world, random, x, y, z, tree_height);
        return;
    }

    place_trunk(world, x, y, z, tree_height - 1, log, meta);

    let mut yy = y - 3 + tree_height;
    while yy <= y + tree_height {
        let y_off = (yy - (y + tree_height)) as f64;
        let mid = (1.0 - y_off / 2.0) as i32;
        for xx in (x - mid)..=(x + mid) {
            let x_off = (xx - x).abs();
            for zz in (z - mid)..=(z + mid) {
                let z_off = (zz - z).abs();
                // 四角按概率镂空 Java 的短路求值顺序要保留
                if x_off == mid && z_off == mid && (y_off == 0.0 || random.next_bounded_int(2) == 0)
                {
                    continue;
                }
                if !is_solid(world.block(xx, yy, zz)) {
                    world.set_block(xx, yy, zz, LEAVES);
                    world.set_meta(xx, yy, zz, meta);
                }
            }
        }
        yy += 1;
    }
}

// 对齐 ObjectSpruceTree.placeObject：一层层收窄的锥形
fn place_spruce(
    world: &mut GenWorld,
    random: &mut NukkitRandom,
    x: i32,
    y: i32,
    z: i32,
    tree_height: i32,
) {
    let (log, meta) = (LOG, WOOD_SPRUCE);
    let top_size = tree_height - (1 + random.next_bounded_int(2));
    let leaf_radius = 1 + random.next_bounded_int(2);

    place_trunk(world, x, y, z, tree_height - 1, log, meta);

    let mut radius = random.next_bounded_int(2);
    let mut max_r = 1;
    let mut min_r = 0;

    for yy in 0..=top_size {
        let yyy = y + tree_height - yy;
        for xx in (x - radius)..=(x + radius) {
            let x_off = (xx - x).abs();
            for zz in (z - radius)..=(z + radius) {
                let z_off = (zz - z).abs();
                if x_off == radius && z_off == radius && radius > 0 {
                    continue;
                }
                if !is_solid(world.block(xx, yyy, zz)) {
                    world.set_block(xx, yyy, zz, LEAVES);
                    world.set_meta(xx, yyy, zz, meta);
                }
            }
        }

        if radius >= max_r {
            radius = min_r;
            min_r = 1;
            max_r += 1;
            if max_r > leaf_radius {
                max_r = leaf_radius;
            }
        } else {
            radius += 1;
        }
    }
}

fn place_trunk(
    world: &mut GenWorld,
    x: i32,
    y: i32,
    z: i32,
    trunk_height: i32,
    log: u8,
    meta: u8,
) {
    // 树干下面垫一层泥土
    world.set_block(x, y - 1, z, DIRT);

    for yy in 0..trunk_height {
        if is_overridable(world.block(x, y + yy, z)) {
            world.set_block(x, y + yy, z, log);
            world.set_meta(x, y + yy, z, meta);
        }
    }
}
