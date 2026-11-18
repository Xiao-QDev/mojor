//! 洞穴 = `LargeFeature::apply` + `LargeCaveFeature`，按 x86 libminecraftpe.so 逐条移植：
//!   LargeFeature::apply            @0x79b9e0   LargeCaveFeature::addFeature @0x7c9900
//!   LargeCaveFeature::addRoom      @0x7c9860   LargeCaveFeature::addTunnel  @0x7c8480
//!   LargeCaveFeature::isDiggable   @0x7c83a0   LargeCaveFeature::_thinSand  @0x7c8350
//!
//! 与 JE MapGenCaves 结构相同，但随机数是 MT19937，`addFeature` 里的参数按**逆序**求值，
//! 每条隧道用 `parent.genrand() >> 1` 给局部随机数播种；全部 f32 运算，顺序照原版。

use crate::level::format::chunk::Chunk;
use crate::level::generator::blocks::{AIR, DIRT, GRASS, SAND, SANDSTONE, STILL_WATER, STONE, WATER};
use crate::level::generator::mcpe_noise::McpeRandom;

const STILL_LAVA: u8 = 11;
const MYCELIUM: u8 = 110;
const STAINED_CLAY: u8 = 159;
const HARDENED_CLAY: u8 = 172;
const RED_SANDSTONE: u8 = 179;
const PODZOL: u8 = 243;

/// `LargeFeature::LargeFeature()` 写入的 range
const RANGE: i32 = 8;
const PI: f32 = std::f32::consts::PI; // Mth::PI = 0x40490fdb

#[inline]
fn nf(u: u32) -> f32 {
    (u as f64 * (1.0 / 4_294_967_296.0)) as f32
}

/// 区块方块读写（原版直接操作 `LevelChunk::getRawBlocks()`）
struct Blocks<'a> {
    chunk: &'a mut Chunk,
}

impl Blocks<'_> {
    #[inline]
    fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        self.chunk.block(x as usize, y as usize, z as usize)
    }
    #[inline]
    fn set(&mut self, x: i32, y: i32, z: i32, id: u8) {
        self.chunk.set_block(x as usize, y as usize, z as usize, id);
    }
    #[inline]
    fn data(&self, x: i32, y: i32, z: i32) -> u8 {
        self.chunk.meta(x as usize, y as usize, z as usize)
    }
}

/// `LargeFeature::apply`：world_seed 为 BiomeSource+0x30（32 位世界种子）
pub fn apply(chunk: &mut Chunk, chunk_x: i32, chunk_z: i32, world_seed: u32) {
    let mut b = Blocks { chunk };
    let mut rnd = McpeRandom::new(world_seed);
    let a = (rnd.genrand() >> 2).wrapping_mul(2).wrapping_add(1);
    let c = (rnd.genrand() >> 2).wrapping_mul(2).wrapping_add(1);
    for x in chunk_x - RANGE..=chunk_x + RANGE {
        for z in chunk_z - RANGE..=chunk_z + RANGE {
            let s = (x as u32).wrapping_mul(a).wrapping_add((z as u32).wrapping_mul(c)) ^ world_seed;
            let mut r = McpeRandom::new(s);
            add_feature(&mut r, &mut b, chunk_x, chunk_z, x, z);
        }
    }
}

/// `LargeCaveFeature::addFeature`
fn add_feature(rnd: &mut McpeRandom, b: &mut Blocks, cx: i32, cz: i32, x: i32, z: i32) {
    let r1 = rnd.genrand();
    let r2 = rnd.genrand();
    let r3 = rnd.genrand();
    let r4 = rnd.genrand();
    if r4 % 15 != 0 {
        return;
    }
    let n = r3 % ((r2 % ((r1 % 40) + 1)) + 1);
    for _ in 0..n {
        let g1 = rnd.genrand();
        let g2 = rnd.genrand();
        let g3 = rnd.genrand();
        let g4 = rnd.genrand();
        let pos = (
            ((g4 & 15) as i32).wrapping_add(x << 4) as f32,
            (g3 % ((g2 % 120) + 8)) as f32,
            ((g1 & 15) as i32).wrapping_add(z << 4) as f32,
        );
        let mut tunnels = 1;
        if rnd.genrand() & 3 == 0 {
            add_room(rnd, b, cx, cz, pos);
            tunnels = (rnd.genrand() & 3) + 1;
        }
        for _ in 0..tunnels {
            let yaw = nf(rnd.genrand()) * (PI + PI);
            let ga = rnd.genrand();
            let gb = rnd.genrand();
            let gc = rnd.genrand();
            let pitch = (nf(ga) + -0.5) * 0.25;
            let tb = nf(gb);
            let thick = (tb + tb) + nf(gc);
            add_tunnel(rnd, b, cx, cz, pos, thick, yaw, pitch, 0, 0, 1.0);
        }
    }
}

/// `LargeCaveFeature::addRoom`
fn add_room(rnd: &mut McpeRandom, b: &mut Blocks, cx: i32, cz: i32, pos: (f32, f32, f32)) {
    let thick = nf(rnd.genrand()) * 6.0 + 1.0;
    add_tunnel(rnd, b, cx, cz, pos, thick, 0.0, 0.0, -1, -1, 0.5);
}

/// `LargeCaveFeature::isDiggable(block, above)`
fn diggable(b: u8, above: u8) -> bool {
    matches!(
        b,
        STONE | DIRT | GRASS | HARDENED_CLAY | STAINED_CLAY | SANDSTONE | RED_SANDSTONE | MYCELIUM | PODZOL
    ) || (b == SAND && above != STILL_WATER)
}

/// `LargeCaveFeature::addTunnel`
#[allow(clippy::too_many_arguments)]
fn add_tunnel(
    parent: &mut McpeRandom,
    b: &mut Blocks,
    cx: i32,
    cz: i32,
    pos: (f32, f32, f32),
    thick: f32,
    mut yaw: f32,
    mut pitch: f32,
    mut step: i32,
    mut end: i32,
    yscale: f32,
) {
    let center_x = ((cx << 4) + 8) as f32;
    let center_z = ((cz << 4) + 8) as f32;
    let (mut px, mut py, mut pz) = pos;
    let mut rnd = McpeRandom::new(parent.genrand() >> 1);
    let mut yaw_d = 0.0f32;
    let mut pitch_d = 0.0f32;

    if end <= 0 {
        let m = (RANGE - 1) << 4;
        let q = m / 4;
        let d = if q != 0 { rnd.genrand() % q as u32 } else { 0 };
        end = m - d as i32;
    }
    let mut is_room = false;
    let mut can_split = true;
    if step == -1 {
        step = end / 2;
        can_split = false;
        is_room = true;
    }
    let half = end / 2;
    let d = if half != 0 { rnd.genrand() % half as u32 } else { 0 };
    let split = end / 4 + d as i32;
    let steep = rnd.genrand() % 6 == 0;
    if step >= end {
        return;
    }

    let tq = thick + 18.0;
    let max_d2 = tq * tq;
    let thick_d = thick as f64;
    let mut rem = end - step;
    let pi_over_end = PI / end as f32;
    let cx16 = 16.0 + center_x;
    let cz16 = 16.0 + center_z;

    loop {
        let sin_v = ((step as f32 * pi_over_end) as f64).sin();
        let cos_p = (pitch as f64).cos();
        let sin_p = (pitch as f64).sin();
        let cos_y = (yaw as f64).cos();
        let cpf = cos_p as f32 as f64;
        px = (cos_y * cpf + px as f64) as f32;
        py += sin_p as f32;
        let sin_y = (yaw as f64).sin();
        pz = (sin_y * cpf + pz as f64) as f32;

        pitch = if steep { 0.92 * pitch } else { 0.7 * pitch } + 0.1 * pitch_d;
        let pd = pitch_d * 0.9;
        yaw += 0.1 * yaw_d;
        let yd = yaw_d * 0.75;
        let r1 = nf(rnd.genrand());
        let r2 = nf(rnd.genrand());
        let t = r1 - r2;
        let r3 = nf(rnd.genrand());
        pitch_d = (r3 + r3) * t + pd;
        let r4 = nf(rnd.genrand());
        let r5 = nf(rnd.genrand());
        let t2 = r4 - r5;
        let r6 = nf(rnd.genrand());
        yaw_d = (r6 * 4.0) * t2 + yd;

        if step == split && can_split && thick > 1.0 {
            let hp = 0.5 * PI;
            let np = pitch * 0.333_333_34;
            let y1 = yaw - hp;
            let t1 = (nf(rnd.genrand()) + 1.0) * 0.5;
            add_tunnel(&mut rnd, b, cx, cz, (px, py, pz), t1, y1, np, step, end, 1.0);
            let y2 = hp + yaw;
            let t2 = (nf(rnd.genrand()) + 1.0) * 0.5;
            add_tunnel(&mut rnd, b, cx, cz, (px, py, pz), t2, y2, np, step, end, 1.0);
            return;
        }

        let skip = !is_room && rnd.genrand() & 3 == 0;
        if !skip {
            let dx = px - center_x;
            let dz = pz - center_z;
            let rf = rem as f32;
            if (dz * dz + dx * dx) - rf * rf > max_d2 {
                return;
            }
            if carve_step(b, cx, cz, px, py, pz, sin_v, thick_d, yscale, center_x, center_z, cx16, cz16) && is_room {
                return;
            }
        }

        step += 1;
        rem -= 1;
        if step == end {
            return;
        }
    }
}

/// 一步挖掘。返回 true 表示"到达了挖掘阶段"（房间此时结束；被区块范围剪枝或遇水返回 false）
#[allow(clippy::too_many_arguments)]
fn carve_step(
    b: &mut Blocks,
    cx: i32,
    cz: i32,
    px: f32,
    py: f32,
    pz: f32,
    sin_v: f64,
    thick_d: f64,
    yscale: f32,
    center_x: f32,
    center_z: f32,
    cx16: f32,
    cz16: f32,
) -> bool {
    let r = (sin_v * thick_d + 1.5) as f32;
    let r2 = r + r;
    if (center_x - r2) - 16.0 > px || (center_z - r2) - 16.0 > pz || px > cx16 + r2 || pz > cz16 + r2 {
        return false;
    }
    let vr = yscale * r;
    let fl = |v: f32| (v as f64).floor() as i32;
    let min_x = (fl(px - r) - (cx << 4) - 1).max(0);
    let max_x = (fl(r + px) - (cx << 4) + 1).min(16);
    let min_y = (fl(py - vr) - 1).max(1);
    let max_y = (fl(py + vr) + 1).min(120);
    let min_z = (fl(pz - r) - (cz << 4) - 1).max(0);
    let max_z = (fl(r + pz) - (cz << 4) + 1).min(16);
    if min_x >= max_x {
        return true;
    }

    // 水检测：边界列检查整段，内部列只看 maxY+1 与 minY-1
    for x in min_x..max_x {
        for z in min_z..max_z {
            let border = x == min_x || x == max_x - 1 || z == min_z || z == max_z - 1;
            let mut y = max_y + 1;
            while y >= min_y - 1 {
                if (0..128).contains(&y) {
                    let id = b.get(x, y, z);
                    if id == WATER || id == STILL_WATER {
                        return false;
                    }
                }
                if !border && y != min_y - 1 {
                    y = min_y - 1;
                } else {
                    y -= 1;
                }
            }
        }
    }

    let inv_r = 1.0f32 / r;
    let inv_vr = 1.0f32 / vr;
    for x in min_x..max_x {
        let fx = ((0.5 - px) + ((cx << 4) + x) as f32) * inv_r;
        let fx2 = fx * fx;
        for z in min_z..max_z {
            let fz = ((0.5 - pz) + ((cz << 4) + z) as f32) * inv_r;
            let t = fz * fz + fx2;
            if !(t < 1.0) {
                continue;
            }
            let mut grass = false;
            let mut y = max_y - 1;
            while y >= min_y {
                let fy = ((0.5 - py) + y as f32) * inv_vr;
                if (fy as f64) > -0.7 && fy * fy + t < 1.0 {
                    // 原版沿用 Java 的写法：判定用 y，实际改的是 y+1 处的方块
                    let bi = y + 1;
                    let cur = b.get(x, bi, z);
                    let above = b.get(x, bi + 1, z);
                    if cur == GRASS {
                        grass = true;
                    }
                    if diggable(cur, above) {
                        if y <= 9 {
                            b.set(x, bi, z, STILL_LAVA);
                        } else {
                            // _thinSand：上方 3 格都是沙子时把上一格换成砂岩（被挖方块数据为 1 → 红砂岩）
                            if y <= 124
                                && b.get(x, bi + 1, z) == SAND
                                && b.get(x, bi + 2, z) == SAND
                                && b.get(x, bi + 3, z) == SAND
                            {
                                let red = b.data(x, bi, z) == 1;
                                b.set(x, bi + 1, z, if red { RED_SANDSTONE } else { SANDSTONE });
                            }
                            b.set(x, bi, z, AIR);
                            if grass && b.get(x, bi - 1, z) == DIRT {
                                b.set(x, bi - 1, z, GRASS);
                            }
                        }
                    }
                }
                y -= 1;
            }
        }
    }
    true
}
