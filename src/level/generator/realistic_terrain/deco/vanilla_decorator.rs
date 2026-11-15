//! 对应 `rtg/deco/VanillaBiomeDecorator.php` + `VanillaBiomeDecorationProfile.php`
//! + `VanillaBiomeDecorationStage.php`。
//!
//! 这是 RTG `BiomeDecorationStage::decorate` 的第二段：在 RTG 自己的 deco 集合
//! 跑完之后，再跑一遍 Minecraft 1.12 原版 BiomeDecorator（矿石 → 沙/黏土/砂砾
//! 斑块 → 树 → 花 → 草 → 枯灌木 → 睡莲 → 蘑菇 → 甘蔗 → 南瓜 → 仙人掌 → 泉水），
//! 外加各 biome 子类的 before/after 钩子。
//!
//! `overrides_hardcoded` 的 biome（平原 1、黑森林 29、繁花森林 132）跳过钩子。

use crate::level::generator::population::block_rules;
use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{offset_pos, surface_height};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::noise::vanilla_simplex::biome_noise;
use crate::level::generator::realistic_terrain::tree::vanilla::{
    generate_bush, generate_cacti, generate_dead_bush, generate_minable, generate_pumpkin,
    generate_reed,
};
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla_trees::{
    generate_vanilla_named_tree, VanillaTreeKind,
};
use crate::level::generator::world::GenWorld;

/// 对应 `VanillaBiomeDecorationProfile::forBiome`。
#[derive(Clone, Debug)]
pub struct VanillaProfile {
    pub kind: &'static str,
    pub trees: i32,
    pub extra_tree_chance: f64,
    pub flowers: i32,
    pub grass: i32,
    pub dead_bushes: i32,
    pub mushrooms: i32,
    pub reeds: i32,
    pub cacti: i32,
    pub waterlilies: i32,
    pub gravel_patches: i32,
    pub sand_patches: i32,
    pub clay_patches: i32,
    pub big_mushrooms: i32,
    pub generate_falls: bool,
    pub allow_vanilla_trees: bool,
    pub overrides_hardcoded: bool,
}

impl Default for VanillaProfile {
    fn default() -> Self {
        Self {
            kind: "base",
            trees: 0,
            extra_tree_chance: 0.1,
            flowers: 2,
            grass: 1,
            dead_bushes: 0,
            mushrooms: 0,
            reeds: 0,
            cacti: 0,
            waterlilies: 0,
            gravel_patches: 1,
            sand_patches: 3,
            clay_patches: 1,
            big_mushrooms: 0,
            generate_falls: true,
            allow_vanilla_trees: true,
            overrides_hardcoded: false,
        }
    }
}

fn in_set(id: u8, set: &[u8]) -> bool {
    set.contains(&id)
}

/// 逐条复刻 PHP 的 biome 判定分支（顺序不能动，因为后面有覆盖钩子）。
pub fn profile_for_biome(biome_id: u8) -> VanillaProfile {
    let mut p = VanillaProfile::default();
    let id = biome_id;

    if in_set(id, &[1, 129]) {
        p.kind = if id == 129 { "sunflower_plains" } else { "plains" };
        p.trees = 0;
        p.extra_tree_chance = 0.05;
        p.flowers = 4;
        p.grass = 10;
    } else if in_set(id, &[2, 17, 130]) {
        p.kind = "desert";
        p.trees = -999;
        p.dead_bushes = 2;
        p.reeds = 50;
        p.cacti = 10;
    } else if in_set(id, &[3, 20, 34, 131, 162]) {
        p.kind = "hills";
        if in_set(id, &[20, 34]) {
            p.trees = 3;
        }
    } else if in_set(id, &[4, 18, 27, 28, 29, 132, 155, 156, 157]) {
        if id == 132 {
            p.kind = "flower_forest";
            p.trees = 6;
            p.flowers = 100;
            p.grass = 1;
        } else if in_set(id, &[27, 28]) {
            p.kind = "birch_forest";
            p.trees = 10;
            p.grass = 2;
        } else if in_set(id, &[155, 156]) {
            p.kind = "mutated_birch_forest";
            p.trees = 10;
            p.grass = 2;
        } else if in_set(id, &[29, 157]) {
            p.kind = "roofed_forest";
            p.trees = -999;
            p.grass = 2;
        } else {
            p.kind = "forest";
            p.trees = 10;
            p.grass = 2;
        }
    } else if in_set(id, &[5, 19, 30, 31, 32, 33, 133, 158, 160, 161]) {
        let mega = in_set(id, &[32, 33, 160, 161]);
        p.kind = if mega {
            if in_set(id, &[160, 161]) {
                "mega_spruce_taiga"
            } else {
                "mega_taiga"
            }
        } else {
            "taiga"
        };
        p.trees = 10;
        p.grass = if mega { 7 } else { 1 };
        p.mushrooms = if mega { 3 } else { 1 };
        if mega {
            p.dead_bushes = 1;
        }
    } else if in_set(id, &[6, 134]) {
        p.kind = "swamp";
        p.trees = 2;
        p.flowers = 1;
        p.dead_bushes = 1;
        p.mushrooms = 8;
        p.reeds = 10;
        p.waterlilies = 4;
        p.sand_patches = 0;
        p.gravel_patches = 0;
        p.grass = 5;
    } else if in_set(id, &[12, 13, 140]) {
        p.kind = if id == 140 { "ice_spikes" } else { "snow" };
    } else if in_set(id, &[14, 15]) {
        p.kind = "mushroom_island";
        p.trees = -100;
        p.flowers = -100;
        p.grass = -100;
        p.mushrooms = 1;
        p.big_mushrooms = 1;
    } else if in_set(id, &[16, 26]) {
        p.kind = "beach";
        p.trees = -999;
        p.dead_bushes = 0;
        p.reeds = 0;
        p.cacti = 0;
    } else if id == 25 {
        p.kind = "stone_beach";
        p.trees = -999;
        p.dead_bushes = 0;
        p.reeds = 0;
        p.cacti = 0;
    } else if in_set(id, &[21, 22, 23, 149, 151]) {
        let edge = in_set(id, &[23, 151]);
        p.kind = if edge { "jungle_edge" } else { "jungle" };
        p.trees = if edge { 2 } else { 50 };
        p.grass = 25;
        p.flowers = 4;
    } else if in_set(id, &[35, 36, 163, 164]) {
        let mutated = in_set(id, &[163, 164]);
        p.kind = if mutated { "mutated_savanna" } else { "savanna" };
        p.trees = if mutated { 2 } else { 1 };
        p.flowers = if mutated { 2 } else { 4 };
        p.grass = if mutated { 5 } else { 20 };
    } else if in_set(id, &[37, 38, 39, 165, 166, 167]) {
        p.kind = "mesa";
        p.trees = if in_set(id, &[38, 166]) { 5 } else { -999 };
        p.dead_bushes = 20;
        p.reeds = 3;
        p.cacti = 5;
        p.flowers = 0;
    } else if in_set(id, &[0, 10, 24]) {
        p.kind = "ocean";
    } else if in_set(id, &[7, 11]) {
        p.kind = "river";
    }

    // RTG realistic-biome hooks：启动时改写了原版装饰器的单例。
    if id == 1 {
        p.grass = 5;
        p.flowers = -999;
        p.trees = -999;
    }
    if id == 29 {
        p.grass = 3;
        p.flowers = 1;
    }
    if id == 132 {
        p.flowers = -999;
    }
    if in_set(
        id,
        &[2, 17, 35, 36, 37, 38, 39, 130, 163, 164, 165, 166, 167],
    ) {
        p.cacti = -999;
    }
    if in_set(id, &[21, 22, 149]) {
        p.trees = 30;
        p.grass = 30;
        p.flowers = -999;
    }
    if in_set(id, &[23, 151]) {
        p.trees = 2;
        p.grass = 30;
        p.flowers = -999;
    }

    p.overrides_hardcoded = in_set(id, &[1, 29, 132]);
    p.allow_vanilla_trees = !in_set(
        id,
        &[
            4, 5, 18, 19, 27, 28, 30, 31, 35, 36, 132, 133, 155, 156, 158, 163, 164,
        ],
    );
    p
}

// ---------------------------------------------------------------------------
// 原版 feature 的独立移植
// ---------------------------------------------------------------------------

/// `WorldGenSand(12,7)` / `(13,6)`：水下沙/砂砾斑块。
pub fn gen_sand(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, block: i32, radius: i32) -> bool {
    if block_rules::material(world.block_state(pos.x, pos.y, pos.z)) != block_rules::WATER {
        return false;
    }
    let r = rand.next_int_bound(radius - 2) + 2;
    for x in (pos.x - r)..=(pos.x + r) {
        for z in (pos.z - r)..=(pos.z + r) {
            let dx = x - pos.x;
            let dz = z - pos.z;
            if dx * dx + dz * dz <= r * r {
                for y in (pos.y - 2)..=(pos.y + 2) {
                    let id = world.block_state(x, y, z) >> 4;
                    if id == 3 || id == 2 || id == 243 {
                        world.set_block_state(x, y, z, block_rules::default_state(block));
                    }
                }
            }
        }
    }
    true
}

/// `WorldGenClay(4)`：水下黏土斑块（方块 82）。
pub fn gen_clay(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, number_of_blocks: i32) -> bool {
    if block_rules::material(world.block_state(pos.x, pos.y, pos.z)) != block_rules::WATER {
        return false;
    }
    let r = rand.next_int_bound(number_of_blocks - 2) + 2;
    for x in (pos.x - r)..=(pos.x + r) {
        for z in (pos.z - r)..=(pos.z + r) {
            let dx = x - pos.x;
            let dz = z - pos.z;
            if dx * dx + dz * dz <= r * r {
                for y in (pos.y - 1)..=(pos.y + 1) {
                    let id = world.block_state(x, y, z) >> 4;
                    if id == 3 || id == 82 || id == 243 {
                        world.set_block_state(x, y, z, block_rules::default_state(82));
                    }
                }
            }
        }
    }
    true
}

/// `WorldGenFlowers`：在 64 次随机偏移中撒花。
pub fn gen_flowers(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, state: i32) -> bool {
    let id = state >> 4;
    for _ in 0..64 {
        let target = Pos::new(
            pos.x + rand.next_int_bound(8) - rand.next_int_bound(8),
            pos.y + rand.next_int_bound(4) - rand.next_int_bound(4),
            pos.z + rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if world.is_air(target.x, target.y, target.z)
            && target.y < 255
            && block_rules::can_block_stay(id, world, target)
        {
            world.set_block_state(target.x, target.y, target.z, state);
        }
    }
    true
}

/// `WorldGenTallGrass`：先向下落到地面，再撒 128 次。
pub fn gen_tall_grass(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, state: i32) -> bool {
    let mut base = pos;
    loop {
        let m = block_rules::material(world.block_state(base.x, base.y, base.z));
        if !(m == block_rules::AIR || m == block_rules::LEAVES) || base.y <= 0 {
            break;
        }
        base = base.down();
    }
    for _ in 0..128 {
        let target = Pos::new(
            base.x + rand.next_int_bound(8) - rand.next_int_bound(8),
            base.y + rand.next_int_bound(4) - rand.next_int_bound(4),
            base.z + rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if world.is_air(target.x, target.y, target.z)
            && block_rules::can_block_stay(31, world, target)
        {
            world.set_block_state(target.x, target.y, target.z, state);
        }
    }
    true
}

/// `WorldGenWaterlily`：10 次尝试放睡莲（1776 = 111<<4）。
pub fn gen_waterlily(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    for _ in 0..10 {
        let x = pos.x + rand.next_int_bound(8) - rand.next_int_bound(8);
        let y = pos.y + rand.next_int_bound(4) - rand.next_int_bound(4);
        let z = pos.z + rand.next_int_bound(8) - rand.next_int_bound(8);
        if world.is_air(x, y, z) && block_rules::can_place_block_at(111, world, Pos::new(x, y, z)) {
            world.set_block_state(x, y, z, 111 << 4);
        }
    }
    true
}

/// `WorldGenDoublePlant`：64 次尝试放双格植物。
pub fn gen_double_plant(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    plant_type: i32,
) -> bool {
    let mut placed = false;
    for _ in 0..64 {
        let target = Pos::new(
            pos.x + rand.next_int_bound(8) - rand.next_int_bound(8),
            pos.y + rand.next_int_bound(4) - rand.next_int_bound(4),
            pos.z + rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if world.is_air(target.x, target.y, target.z)
            && target.y < 254
            && block_rules::can_place_block_at(175, world, target)
        {
            block_rules::place_at(175, world, target, plant_type);
            placed = true;
        }
    }
    placed
}

/// `WorldGenMelon`：64 次尝试，要求下方是草方块。
pub fn gen_melon(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    for _ in 0..64 {
        let target = Pos::new(
            pos.x + rand.next_int_bound(8) - rand.next_int_bound(8),
            pos.y + rand.next_int_bound(4) - rand.next_int_bound(4),
            pos.z + rand.next_int_bound(8) - rand.next_int_bound(8),
        );
        if block_rules::can_place_block_at(103, world, target)
            && world.block_state(target.x, target.y - 1, target.z) >> 4 == 2
        {
            world.set_block_state(target.x, target.y, target.z, 1648);
        }
    }
    true
}

/// `BlockRules::isAcceptableVineNeighbour`。
fn acceptable_vine_neighbour(state: i32) -> bool {
    let id = state >> 4;
    if (219..=234).contains(&id) || matches!(id, 29 | 33 | 34 | 96 | 118 | 138) {
        return false;
    }
    block_rules::is_full_block(state)
}

/// `WorldGenVines`：从给定 y 一路向上，贴墙放藤蔓。
pub fn gen_vines(world: &mut GenWorld, rand: &mut JavaRandom, mut pos: Pos, max_y: i32) -> bool {
    while pos.y < max_y {
        if world.is_air(pos.x, pos.y, pos.z) {
            // Facing::horizontal() = NORTH, EAST, SOUTH, WEST
            for (face, dx, dz) in [("north", 0, -1), ("east", 1, 0), ("south", 0, 1), ("west", -1, 0)] {
                let neighbour = world.block_state(pos.x + dx, pos.y, pos.z + dz);
                if !acceptable_vine_neighbour(neighbour) {
                    continue;
                }
                let above = world.block_state(pos.x, pos.y + 1, pos.z) >> 4;
                if above == 0 || above == 106 || acceptable_vine_neighbour(above) {
                    world.set_block_state(pos.x, pos.y, pos.z, block_rules::vine_with_face(face));
                    break;
                }
            }
            pos = pos.up();
        } else {
            pos = Pos::new(
                pos.x + rand.next_int_bound(4) - rand.next_int_bound(4),
                pos.y,
                pos.z + rand.next_int_bound(4) - rand.next_int_bound(4),
            );
        }
    }
    true
}

/// `WorldGenLiquids`：在四周 3 面石头、1 面空气的缝隙里放泉水。
pub fn gen_liquid(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, block: i32) -> bool {
    let _ = rand;
    if world.block_state(pos.x, pos.y + 1, pos.z) >> 4 != 1 {
        return false;
    }
    if world.block_state(pos.x, pos.y - 1, pos.z) >> 4 != 1 {
        return false;
    }
    let state = world.block_state(pos.x, pos.y, pos.z);
    if !(block_rules::material(state) == block_rules::AIR || state >> 4 == 1) {
        return false;
    }
    let mut stone_sides = 0;
    let mut air_sides = 0;
    for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
        if world.block_state(pos.x + dx, pos.y, pos.z + dz) >> 4 == 1 {
            stone_sides += 1;
        }
        if world.is_air(pos.x + dx, pos.y, pos.z + dz) {
            air_sides += 1;
        }
    }
    if stone_sides == 3 && air_sides == 1 {
        world.set_block_state(pos.x, pos.y, pos.z, block_rules::default_state(block));
    }
    true
}

/// `WorldGenIcePath(4)`：冰面小径。
pub fn gen_ice_path(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, base_width: i32) -> bool {
    let mut base = pos;
    while world.is_air(base.x, base.y, base.z) && base.y > 2 {
        base = base.down();
    }
    if world.block_state(base.x, base.y, base.z) >> 4 != 80 {
        return false;
    }
    let r = rand.next_int_bound(base_width - 2) + 2;
    for x in (base.x - r)..=(base.x + r) {
        for z in (base.z - r)..=(base.z + r) {
            let dx = x - base.x;
            let dz = z - base.z;
            if dx * dx + dz * dz <= r * r {
                for y in (base.y - 1)..=(base.y + 1) {
                    let id = world.block_state(x, y, z) >> 4;
                    if id == 3 || id == 80 || id == 79 || id == 243 {
                        world.set_block_state(x, y, z, 174 << 4);
                    }
                }
            }
        }
    }
    true
}

/// `WorldGenIceSpike`（原版，非 RTG 的冰刺树）。
pub fn gen_ice_spike(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let mut base = pos;
    while world.is_air(base.x, base.y, base.z) && base.y > 2 {
        base = base.down();
    }
    if world.block_state(base.x, base.y, base.z) >> 4 != 80 {
        return false;
    }
    base = base.up_n(rand.next_int_bound(4));
    let height = rand.next_int_bound(4) + 7;
    let width = height / 4 + rand.next_int_bound(2);
    if width > 1 && rand.next_int_bound(60) == 0 {
        base = base.up_n(10 + rand.next_int_bound(30));
    }

    let ice = 174 << 4;
    for step in 0..height {
        let fr = f(f(1.0 - f(f(step as f64) / f(height as f64))) * f(width as f64));
        let radius = fr.ceil() as i32;
        for dx in -radius..=radius {
            let fx = f(f(dx.abs() as f64) - f(0.25));
            for dz in -radius..=radius {
                let fz = f(f(dz.abs() as f64) - f(0.25));
                let inner = dx == 0 && dz == 0;
                let inside = f(f(fx * fx) + f(fz * fz)) <= f(fr * fr);
                let edge_ok = ((dx != -radius && dx != radius) && (dz != -radius && dz != radius))
                    || (rand.next_float() as f64) <= f(0.75);
                if !((inner || inside) && edge_ok) {
                    continue;
                }
                let target = base.add(dx, step, dz);
                let state = world.block_state(target.x, target.y, target.z);
                let id = state >> 4;
                if block_rules::material(state) == block_rules::AIR
                    || id == 3
                    || id == 80
                    || id == 79
                    || id == 243
                {
                    world.set_block_state(target.x, target.y, target.z, ice);
                }
                if step != 0 && radius > 1 {
                    let below = base.add(dx, -step, dz);
                    let state = world.block_state(below.x, below.y, below.z);
                    let id = state >> 4;
                    if block_rules::material(state) == block_rules::AIR
                        || id == 3
                        || id == 80
                        || id == 79
                        || id == 243
                    {
                        world.set_block_state(below.x, below.y, below.z, ice);
                    }
                }
            }
        }
    }

    let mut k1 = width - 1;
    if k1 < 0 {
        k1 = 0;
    } else if k1 > 1 {
        k1 = 1;
    }
    for dx in -k1..=k1 {
        for dz in -k1..=k1 {
            let mut target = base.add(dx, -1, dz);
            let mut remaining = 50;
            if dx.abs() == 1 && dz.abs() == 1 {
                remaining = rand.next_int_bound(5);
            }
            while target.y > 50 {
                let state = world.block_state(target.x, target.y, target.z);
                let id = state >> 4;
                if block_rules::material(state) != block_rules::AIR
                    && id != 3
                    && id != 80
                    && id != 79
                    && id != 174
                {
                    break;
                }
                world.set_block_state(target.x, target.y, target.z, ice);
                target = target.down();
                remaining -= 1;
                if remaining <= 0 {
                    target = target.down_n(rand.next_int_bound(5) + 1);
                    remaining = rand.next_int_bound(5);
                }
            }
        }
    }
    true
}

/// `WorldGenDesertWells`：沙漠水井。
pub fn gen_desert_well(world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
    let _ = rand;
    const SAND_SLAB: i32 = 705;
    const SANDSTONE: i32 = 384;
    const WATER: i32 = 128;
    let mut base = pos;
    while world.is_air(base.x, base.y, base.z) && base.y > 2 {
        base = base.down();
    }
    if world.block_state(base.x, base.y, base.z) != (12 << 4) {
        return false;
    }
    for dx in -2..=2 {
        for dz in -2..=2 {
            if world.is_air(base.x + dx, base.y - 1, base.z + dz)
                && world.is_air(base.x + dx, base.y - 2, base.z + dz)
            {
                return false;
            }
        }
    }
    for dy in -1..=0 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                world.set_block_state(base.x + dx, base.y + dy, base.z + dz, SANDSTONE);
            }
        }
    }
    world.set_block_state(base.x, base.y, base.z, WATER);
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        world.set_block_state(base.x + dx, base.y, base.z + dz, WATER);
    }
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            if dx.abs() == 2 || dz.abs() == 2 {
                world.set_block_state(base.x + dx, base.y + 1, base.z + dz, SANDSTONE);
            }
        }
    }
    for (dx, dz) in [(2, 0), (-2, 0), (0, 2), (0, -2)] {
        world.set_block_state(base.x + dx, base.y + 1, base.z + dz, SAND_SLAB);
    }
    for dx in -1..=1 {
        for dz in -1..=1 {
            let state = if dx == 0 && dz == 0 { SANDSTONE } else { SAND_SLAB };
            world.set_block_state(base.x + dx, base.y + 4, base.z + dz, state);
        }
    }
    for dy in 1..=3 {
        for (dx, dz) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
            world.set_block_state(base.x + dx, base.y + dy, base.z + dz, SANDSTONE);
        }
    }
    true
}

/// `WorldGenBlockBlob`：苔石团（大针叶林用，`state` 已左移 4 位）。
pub fn gen_block_blob(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    state: i32,
    start_radius: i32,
) -> bool {
    let mut base = pos;
    loop {
        if base.y <= 3 {
            return false;
        }
        if !world.is_air(base.x, base.y - 1, base.z) {
            let below = world.block_state(base.x, base.y - 1, base.z) >> 4;
            if matches!(below, 1 | 2 | 3) {
                break;
            }
        }
        base = base.down();
    }
    let radius = start_radius;
    for _ in 0..3 {
        if radius < 0 {
            break;
        }
        let rx = radius + rand.next_int_bound(2);
        let ry = radius + rand.next_int_bound(2);
        let rz = radius + rand.next_int_bound(2);
        // PHP 这里是普通 double 运算，不做 f32 截断。
        let limit = (rx + ry + rz) as f64 * 0.333_000_004_291_534_4 + 0.5;
        let limit2 = limit * limit;
        for x in (base.x - rx)..=(base.x + rx) {
            for y in (base.y - ry)..=(base.y + ry) {
                for z in (base.z - rz)..=(base.z + rz) {
                    let dx = (x - base.x) as f64;
                    let dy = (y - base.y) as f64;
                    let dz = (z - base.z) as f64;
                    if dx * dx + dy * dy + dz * dz <= limit2 {
                        world.set_block_state(x, y, z, state);
                    }
                }
            }
        }
        base = Pos::new(
            base.x - (radius + 1) + rand.next_int_bound(2 + radius * 2),
            base.y - rand.next_int_bound(2),
            base.z - (radius + 1) + rand.next_int_bound(2 + radius * 2),
        );
    }
    true
}

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 对应 `VanillaBiomeDecorationStage::generate`。
pub fn generate_vanilla_decoration(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    chunk_x: i32,
    chunk_z: i32,
    biome_id: u8,
) {
    let mut p = profile_for_biome(biome_id);
    let origin = offset_pos(chunk_x, chunk_z);

    if !p.overrides_hardcoded {
        before_decorator(world, rand, origin, &mut p);
    }
    decorate(world, rand, origin, &p);
    if !p.overrides_hardcoded {
        after_decorator(world, rand, origin, &p);
    }
}

fn generate_at_random_height(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    origin: Pos,
    kind: RandomHeightFeature,
) {
    let x = rand.next_int_bound(16) + 8;
    let z = rand.next_int_bound(16) + 8;
    let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
    if bound > 0 {
        let y = rand.next_int_bound(bound);
        let pos = Pos::new(origin.x + x, y, origin.z + z);
        match kind {
            RandomHeightFeature::Reed => {
                generate_reed(world, rand, pos);
            }
            RandomHeightFeature::Pumpkin => {
                generate_pumpkin(world, rand, pos);
            }
            RandomHeightFeature::Cactus => {
                // 原版 WorldGenCactus()：只长在沙子上，无额外高度。
                generate_cacti(world, rand, pos, true, 0, 12 << 4);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RandomHeightFeature {
    Reed,
    Pumpkin,
    Cactus,
}

/// `VanillaBiomeDecorator::generate` 主体。
fn decorate(world: &mut GenWorld, rand: &mut JavaRandom, origin: Pos, p: &VanillaProfile) {
    // 1) 矿石（biome 无关）
    let ore = crate::level::generator::realistic_terrain::deco::OreDecoration::default();
    ore.generate(world, rand, origin.x >> 4, origin.z >> 4);

    // 2) 沙 / 黏土 / 砂砾斑块
    for _ in 0..p.sand_patches.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let y = surface_height(world, origin.x + x, origin.z + z);
        gen_sand(world, rand, Pos::new(origin.x + x, y, origin.z + z), 12, 7);
    }
    for _ in 0..p.clay_patches.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let y = surface_height(world, origin.x + x, origin.z + z);
        gen_clay(world, rand, Pos::new(origin.x + x, y, origin.z + z), 4);
    }
    for _ in 0..p.gravel_patches.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let y = surface_height(world, origin.x + x, origin.z + z);
        gen_sand(world, rand, Pos::new(origin.x + x, y, origin.z + z), 13, 6);
    }

    // 3) 树
    let mut trees = p.trees;
    let mut extra = p.extra_tree_chance;
    if !p.overrides_hardcoded && !p.allow_vanilla_trees {
        trees = 0;
        extra = 0.0;
    }
    if (rand.next_float() as f64) < extra {
        trees += 1;
    }
    for _ in 0..trees.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let kind = tree_for(p.kind, rand);
        let y = surface_height(world, origin.x + x, origin.z + z);
        let pos = Pos::new(origin.x + x, y, origin.z + z);
        if generate_vanilla_named_tree(world, rand, pos, kind) {
            // generateSaplings：这些原版树都没有树苗，保持空实现。
        }
    }

    // 4) 大蘑菇
    for _ in 0..p.big_mushrooms.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let y = surface_height(world, origin.x + x, origin.z + z);
        let pos = Pos::new(origin.x + x, y, origin.z + z);
        let mut shroom =
            crate::level::generator::realistic_terrain::tree::WorldGenBigMushroom::default();
        use crate::level::generator::realistic_terrain::tree::WorldGen;
        shroom.generate(world, rand, pos);
    }

    // 5) 花
    for _ in 0..p.flowers.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) + 32;
        if bound > 0 {
            let y = rand.next_int_bound(bound);
            let pos = Pos::new(origin.x + x, y, origin.z + z);
            let flower_type = flower_for(p.kind, rand, pos);
            let state = if flower_type == 0 {
                37 << 4
            } else {
                (38 << 4) | ((flower_type - 1) & 15)
            };
            gen_flowers(world, rand, pos, state);
        }
    }

    // 6) 草
    for _ in 0..p.grass.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        if bound > 0 {
            let y = rand.next_int_bound(bound);
            let variant = grass_variant_for(p.kind, rand);
            gen_tall_grass(world, rand, Pos::new(origin.x + x, y, origin.z + z), (31 << 4) | variant);
        }
    }

    // 7) 枯灌木
    for _ in 0..p.dead_bushes.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        if bound > 0 {
            let y = rand.next_int_bound(bound);
            generate_dead_bush(world, rand, Pos::new(origin.x + x, y, origin.z + z));
        }
    }

    // 8) 睡莲
    for _ in 0..p.waterlilies.max(0) {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        if bound > 0 {
            let mut pos = Pos::new(origin.x + x, rand.next_int_bound(bound), origin.z + z);
            while pos.y > 0 {
                let below = pos.down();
                if !world.is_air(below.x, below.y, below.z) {
                    break;
                }
                pos = below;
            }
            gen_waterlily(world, rand, pos);
        }
    }

    // 9) 蘑菇（profile 计数 + 无条件的棕色/红色蘑菇）
    for _ in 0..p.mushrooms.max(0) {
        if rand.next_int_bound(4) == 0 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let y = surface_height(world, origin.x + x, origin.z + z);
            generate_bush(world, rand, Pos::new(origin.x + x, y, origin.z + z), 39);
        }
        if rand.next_int_bound(8) == 0 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
            if bound > 0 {
                let y = rand.next_int_bound(bound);
                generate_bush(world, rand, Pos::new(origin.x + x, y, origin.z + z), 40);
            }
        }
    }
    if rand.next_int_bound(4) == 0 {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        if bound > 0 {
            let y = rand.next_int_bound(bound);
            generate_bush(world, rand, Pos::new(origin.x + x, y, origin.z + z), 39);
        }
    }
    if rand.next_int_bound(8) == 0 {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        if bound > 0 {
            let y = rand.next_int_bound(bound);
            generate_bush(world, rand, Pos::new(origin.x + x, y, origin.z + z), 40);
        }
    }

    // 10) 甘蔗：profile 次数 + 固定 10 次
    for _ in 0..p.reeds.max(0) {
        generate_at_random_height(world, rand, origin, RandomHeightFeature::Reed);
    }
    for _ in 0..10 {
        generate_at_random_height(world, rand, origin, RandomHeightFeature::Reed);
    }

    // 11) 南瓜：1/32
    if rand.next_int_bound(32) == 0 {
        generate_at_random_height(world, rand, origin, RandomHeightFeature::Pumpkin);
    }

    // 12) 仙人掌
    for _ in 0..p.cacti.max(0) {
        generate_at_random_height(world, rand, origin, RandomHeightFeature::Cactus);
    }

    // 13) 泉水
    if p.generate_falls {
        for _ in 0..50 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let upper = rand.next_int_bound(248) + 8;
            let y = rand.next_int_bound(upper);
            gen_liquid(world, rand, Pos::new(origin.x + x, y, origin.z + z), 8);
        }
        for _ in 0..20 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let inner1 = rand.next_int_bound(240) + 8;
            let inner2 = rand.next_int_bound(inner1) + 8;
            let y = rand.next_int_bound(inner2);
            gen_liquid(world, rand, Pos::new(origin.x + x, y, origin.z + z), 10);
        }
    }
}

/// `VanillaBiomeDecorator::grassFor`。
fn grass_variant_for(kind: &str, rand: &mut JavaRandom) -> i32 {
    if kind == "jungle" || kind == "jungle_edge" {
        // 1/4 蕨(2)，否则草(1)
        return if rand.next_int_bound(4) == 0 { 2 } else { 1 };
    }
    if matches!(kind, "taiga" | "mega_taiga" | "mega_spruce_taiga") {
        return if rand.next_int_bound(5) > 0 { 2 } else { 1 };
    }
    1
}

/// `VanillaBiomeDecorator::flowerFor`。
fn flower_for(kind: &str, rand: &mut JavaRandom, pos: Pos) -> i32 {
    if kind == "swamp" {
        return 2;
    }
    if kind == "sunflower_plains" {
        let noise = biome_noise(pos.x as f64 / 200.0, pos.z as f64 / 200.0);
        if noise < -0.8 {
            return [6, 5, 8, 7][rand.next_int_bound(4) as usize];
        }
        if rand.next_int_bound(3) > 0 {
            let pick = rand.next_int_bound(3);
            return if pick == 0 {
                1
            } else if pick == 1 {
                4
            } else {
                9
            };
        }
        return 0;
    }
    if rand.next_int_bound(3) > 0 {
        0
    } else {
        1
    }
}

/// `VanillaBiomeDecorator::treeFor`。
fn tree_for(kind: &str, rand: &mut JavaRandom) -> VanillaTreeKind {
    match kind {
        "plains" | "sunflower_plains" => {
            if rand.next_int_bound(3) == 0 {
                VanillaTreeKind::BigOak
            } else {
                VanillaTreeKind::Oak
            }
        }
        "hills" => {
            if rand.next_int_bound(3) > 0 {
                VanillaTreeKind::Taiga2
            } else {
                base_tree(rand)
            }
        }
        "forest" | "flower_forest" => {
            if rand.next_int_bound(5) != 0 {
                if rand.next_int_bound(10) == 0 {
                    VanillaTreeKind::BigOak
                } else {
                    VanillaTreeKind::Oak
                }
            } else {
                VanillaTreeKind::Birch
            }
        }
        "birch_forest" => VanillaTreeKind::Birch,
        "mutated_birch_forest" => {
            if rand.next_boolean() {
                VanillaTreeKind::TallBirch
            } else {
                VanillaTreeKind::Birch
            }
        }
        "roofed_forest" => {
            if rand.next_int_bound(3) > 0 {
                VanillaTreeKind::Canopy
            } else if rand.next_int_bound(5) != 0 {
                if rand.next_int_bound(10) == 0 {
                    VanillaTreeKind::BigOak
                } else {
                    VanillaTreeKind::Oak
                }
            } else {
                VanillaTreeKind::Birch
            }
        }
        "swamp" => VanillaTreeKind::Swamp,
        "snow" | "ice_spikes" => VanillaTreeKind::Taiga2,
        "taiga" => {
            if rand.next_int_bound(3) == 0 {
                VanillaTreeKind::Taiga1
            } else {
                VanillaTreeKind::Taiga2
            }
        }
        "mega_taiga" | "mega_spruce_taiga" => {
            if rand.next_int_bound(3) == 0 {
                if kind != "mega_spruce_taiga" && rand.next_int_bound(13) != 0 {
                    VanillaTreeKind::MegaPine
                } else {
                    VanillaTreeKind::MegaSpruce
                }
            } else if rand.next_int_bound(3) == 0 {
                VanillaTreeKind::Taiga1
            } else {
                VanillaTreeKind::Taiga2
            }
        }
        "jungle" | "jungle_edge" => {
            if rand.next_int_bound(10) == 0 {
                VanillaTreeKind::BigOak
            } else if rand.next_int_bound(2) == 0 {
                VanillaTreeKind::JungleShrub
            } else if kind == "jungle" && rand.next_int_bound(3) == 0 {
                VanillaTreeKind::MegaJungle
            } else {
                VanillaTreeKind::Jungle
            }
        }
        "savanna" | "mutated_savanna" => {
            if rand.next_int_bound(5) > 0 {
                VanillaTreeKind::Savanna
            } else {
                VanillaTreeKind::Oak
            }
        }
        "mesa" => VanillaTreeKind::Oak,
        _ => base_tree(rand),
    }
}

fn base_tree(rand: &mut JavaRandom) -> VanillaTreeKind {
    if rand.next_int_bound(10) == 0 {
        VanillaTreeKind::BigOak
    } else {
        VanillaTreeKind::Oak
    }
}

/// `VanillaBiomeDecorationStage::beforeDecorator`。
fn before_decorator(world: &mut GenWorld, rand: &mut JavaRandom, origin: Pos, p: &mut VanillaProfile) {
    let kind = p.kind;

    if kind == "ice_spikes" {
        for _ in 0..3 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let y = surface_height(world, origin.x + x, origin.z + z);
            gen_ice_spike(world, rand, Pos::new(origin.x + x, y, origin.z + z));
        }
        for _ in 0..2 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let y = surface_height(world, origin.x + x, origin.z + z);
            gen_ice_path(world, rand, Pos::new(origin.x + x, y, origin.z + z), 4);
        }
    }

    if matches!(
        kind,
        "forest" | "birch_forest" | "mutated_birch_forest" | "flower_forest" | "roofed_forest"
    ) {
        if kind == "roofed_forest" {
            add_roofed_forest_trees(world, rand, origin, kind);
        }
        let loops = rand.next_int_bound(5) - 3 + if kind == "flower_forest" { 2 } else { 0 };
        add_forest_double_plants(world, rand, origin, loops);
    }

    if matches!(kind, "taiga" | "mega_taiga" | "mega_spruce_taiga") {
        if kind != "taiga" {
            let loops = rand.next_int_bound(3);
            for _ in 0..loops {
                let x = rand.next_int_bound(16) + 8;
                let z = rand.next_int_bound(16) + 8;
                let y = surface_height(world, origin.x + x, origin.z + z);
                gen_block_blob(world, rand, Pos::new(origin.x + x, y, origin.z + z), 48 << 4, 0);
            }
        }
        for _ in 0..7 {
            try_double_plant(world, rand, origin, 3);
        }
    }

    if kind == "savanna" {
        for _ in 0..7 {
            try_double_plant(world, rand, origin, 2);
        }
    }

    if kind == "sunflower_plains" {
        let noise = biome_noise((origin.x + 8) as f64 / 200.0, (origin.z + 8) as f64 / 200.0);
        if noise < -0.8 {
            p.flowers = 15;
            p.grass = 5;
        } else {
            p.flowers = 4;
            p.grass = 10;
            for _ in 0..7 {
                try_double_plant(world, rand, origin, 2);
            }
        }
        for _ in 0..10 {
            try_double_plant(world, rand, origin, 0);
        }
    }
}

/// `VanillaBiomeDecorationStage::afterDecorator`。
fn after_decorator(world: &mut GenWorld, rand: &mut JavaRandom, origin: Pos, p: &VanillaProfile) {
    let kind = p.kind;
    if kind == "hills" {
        let loops = 3 + rand.next_int_bound(6);
        for _ in 0..loops {
            let pos = Pos::new(
                origin.x + rand.next_int_bound(16),
                rand.next_int_bound(28) + 4,
                origin.z + rand.next_int_bound(16),
            );
            if world.block_state(pos.x, pos.y, pos.z) >> 4 == 1 {
                world.set_block_state(pos.x, pos.y, pos.z, 129 << 4);
            }
        }
        for _ in 0..7 {
            let pos = Pos::new(
                origin.x + rand.next_int_bound(16),
                rand.next_int_bound(64),
                origin.z + rand.next_int_bound(16),
            );
            generate_minable(world, rand, pos, 97 << 4, 9);
        }
    }

    if kind == "jungle" || kind == "jungle_edge" {
        let x = rand.next_int_bound(16) + 8;
        let z = rand.next_int_bound(16) + 8;
        let bound = surface_height(world, origin.x + x, origin.z + z) * 2;
        // PHP 无条件调用 nextInt(bound)，这里保持同样的 RNG 消耗。
        let y = rand.next_int_bound(bound.max(1));
        gen_melon(world, rand, Pos::new(origin.x + x, y, origin.z + z));
        // 原版从 y=128 起向上找墙挂藤蔓；本移植世界高度 128，改成从顶部往下检查。
        let top = crate::level::format::chunk::CHUNK_HEIGHT as i32;
        for _ in 0..50 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            gen_vines(world, rand, Pos::new(origin.x + x, top - 1, origin.z + z), top);
        }
    }

    if kind == "desert" {
        if rand.next_int_bound(1000) == 0 {
            let x = rand.next_int_bound(16) + 8;
            let z = rand.next_int_bound(16) + 8;
            let y = surface_height(world, origin.x + x, origin.z + z);
            gen_desert_well(world, rand, Pos::new(origin.x + x, y + 1, origin.z + z));
        }
        if rand.next_int_bound(64) == 0 {
            // WorldGenFossils 需要模板数据，暂未移植；RNG 由世界种子单独派生，
            // 跳过它不影响 population RNG 的消耗。
        }
    } else if kind == "swamp" && rand.next_int_bound(64) == 0 {
        // 同上：化石暂未移植。
    }
}

fn try_double_plant(world: &mut GenWorld, rand: &mut JavaRandom, origin: Pos, plant_type: i32) -> bool {
    let x = rand.next_int_bound(16) + 8;
    let z = rand.next_int_bound(16) + 8;
    let bound = surface_height(world, origin.x + x, origin.z + z) + 32;
    let y = rand.next_int_bound(bound);
    gen_double_plant(world, rand, Pos::new(origin.x + x, y, origin.z + z), plant_type)
}

fn add_forest_double_plants(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    origin: Pos,
    loops: i32,
) {
    for _ in 0..loops.max(0) {
        let plant_type = [1, 4, 5][rand.next_int_bound(3) as usize];
        for _ in 0..5 {
            if try_double_plant(world, rand, origin, plant_type) {
                break;
            }
        }
    }
}

fn add_roofed_forest_trees(world: &mut GenWorld, rand: &mut JavaRandom, origin: Pos, kind: &str) {
    use crate::level::generator::realistic_terrain::tree::{WorldGen, WorldGenBigMushroom};
    for gx in 0..4 {
        for gz in 0..4 {
            let x = gx * 4 + 9 + rand.next_int_bound(3);
            let z = gz * 4 + 9 + rand.next_int_bound(3);
            let y = surface_height(world, origin.x + x, origin.z + z);
            let pos = Pos::new(origin.x + x, y, origin.z + z);
            if rand.next_int_bound(20) == 0 {
                let mut shroom = WorldGenBigMushroom::default();
                shroom.generate(world, rand, pos);
            } else {
                let tree_kind = tree_for(kind, rand);
                generate_vanilla_named_tree(world, rand, pos, tree_kind);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    /// 原版装饰器用 +8 偏移，会写到相邻区块，所以测试要铺 4x4 个区块的草地。
    fn grass_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for cx in -1..=2 {
            for cz in -1..=2 {
                chunks.insert((cx, cz), Chunk::empty(cx, cz));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -16..48 {
            for z in -16..48 {
                world.set_block_state(x, 63, z, 2 << 4); // 草地
            }
        }
        world
    }

    #[test]
    fn profile_matches_php_overrides() {
        let plains = profile_for_biome(1);
        assert_eq!(plains.kind, "plains");
        assert_eq!(plains.grass, 5);
        assert_eq!(plains.flowers, -999);
        assert_eq!(plains.trees, -999);
        assert!(plains.overrides_hardcoded);

        let jungle = profile_for_biome(21);
        assert_eq!(jungle.kind, "jungle");
        assert_eq!((jungle.trees, jungle.grass, jungle.flowers), (30, 30, -999));

        let mushroom = profile_for_biome(14);
        assert_eq!(mushroom.trees, -100);
        assert_eq!(mushroom.big_mushrooms, 1);

        let forest = profile_for_biome(4);
        assert!(!forest.allow_vanilla_trees);

        let swamp = profile_for_biome(6);
        assert_eq!(swamp.kind, "swamp");
        assert_eq!(swamp.waterlilies, 4);
        assert_eq!(swamp.sand_patches, 0);
    }

    #[test]
    fn plains_generates_tall_grass() {
        let mut world = grass_world();
        let mut rand = JavaRandom::new(2024);
        generate_vanilla_decoration(&mut world, &mut rand, 0, 0, 1);

        let grass = (-16..48)
            .flat_map(|x| (-16..48).map(move |z| (x, z)))
            .filter(|&(x, z)| {
                let s = world.block_state(x, 64, z);
                s >> 4 == 31 && matches!(s & 15, 1 | 2)
            })
            .count();
        assert!(grass > 0, "平原应生成高草");
        // 平原 flowers=-999，不应有花
        let flowers = (-16..48)
            .flat_map(|x| (-16..48).map(move |z| (x, z)))
            .filter(|&(x, z)| matches!(world.block_state(x, 64, z) >> 4, 37 | 38))
            .count();
        assert_eq!(flowers, 0, "平原不应生成花");
    }
}
