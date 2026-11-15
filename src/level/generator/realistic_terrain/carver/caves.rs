//! 对应 `rtg/carver/MapGenCaves.php` + `MapGenCavesRTG.php`。
//!
//! Mojang 的 `MapGenCaves`（1.7/1.8 版）逐行翻译，`nextInt(15)/nextInt(7)`
//! 换成 `caveDensity/caveChance`（vanilla 预设下数值相同）。
//!
//! `add_tunnel` 是同一个递归函数：主隧道、分叉、以及 `add_room` 生成的球形空洞。
//! 每段隧道按 `total` 步推进，离 origin 太远时提前剪枝。

use crate::level::generator::realistic_terrain::carver::CarverWorld;
use crate::level::generator::realistic_terrain::carver::math_helper::{cos, floor, sin};
use crate::level::generator::realistic_terrain::chunk_primer::ChunkPrimer;
use crate::level::generator::realistic_terrain::math::{JavaRandom, f};

/// `id 11 << 4`：洞穴底部灌岩浆
const BLK_LAVA: i32 = 176;

/// π 的 float32 值（PHP 里反复写 `F::f(3.141592653589793)`）
const PI_F: f64 = std::f32::consts::PI as f64;

pub struct Caves {
    /// `MapGenBase::$range`，固定 8
    pub range: i32,
    rand: JavaRandom,
    cave_chance: i32,
    cave_density: i32,
}

impl Caves {
    pub fn new(cave_chance: i32, cave_density: i32) -> Self {
        assert!(
            cave_chance >= 1,
            "caveChance 必须 >= 1（PHP 里 nextInt(0) 会崩）"
        );
        assert!(cave_density >= 1, "caveDensity 必须 >= 1");
        Self {
            range: 8,
            rand: JavaRandom::new(0),
            cave_chance,
            cave_density,
        }
    }

    /// `MapGenBase::generate`：遍历 17×17 个 origin，各自独立播种。
    pub fn generate(
        &mut self,
        world: &CarverWorld,
        chunk_x: i32,
        chunk_z: i32,
        primer: &mut ChunkPrimer,
    ) {
        let seed = world.seed();
        self.rand.set_seed(seed);
        let a = self.rand.next_long();
        let b = self.rand.next_long();

        for origin_x in chunk_x - self.range..=chunk_x + self.range {
            let ax = (origin_x as i64).wrapping_mul(a);
            for origin_z in chunk_z - self.range..=chunk_z + self.range {
                self.rand
                    .set_seed(ax ^ (origin_z as i64).wrapping_mul(b) ^ seed);
                self.recursive_generate(world, origin_x, origin_z, chunk_x, chunk_z, primer);
            }
        }
    }

    /// `MapGenCavesRTG::recursiveGenerate`
    fn recursive_generate(
        &mut self,
        world: &CarverWorld,
        chunk_x: i32,
        chunk_z: i32,
        origin_x: i32,
        origin_z: i32,
        primer: &mut ChunkPrimer,
    ) {
        // nextInt(nextInt(nextInt(caveDensity) + 1) + 1)
        let a = self.rand.next_int_bound(self.cave_density);
        let b = self.rand.next_int_bound(a + 1);
        let mut density = self.rand.next_int_bound(b + 1);
        if self.rand.next_int_bound(self.cave_chance) != 0 {
            density = 0;
        }

        for _ in 0..density {
            let x = (chunk_x * 16 + self.rand.next_int_bound(16)) as f64;
            let depth_bound = self.rand.next_int_bound(120) + 8;
            let y = self.rand.next_int_bound(depth_bound) as f64;
            let z = (chunk_z * 16 + self.rand.next_int_bound(16)) as f64;

            let mut count = 1;
            if self.rand.next_int_bound(4) == 0 {
                self.add_room(world, origin_x, origin_z, primer, x, y, z);
                count += self.rand.next_int_bound(4);
            }

            for _ in 0..count {
                let yaw = f(self.rand.next_float() as f64 * f(PI_F * 2.0));
                let pitch = f(f((self.rand.next_float() as f64 - 0.5) * 2.0) / 8.0);
                let mut thickness =
                    f(self.rand.next_float() as f64 * 2.0 + self.rand.next_float() as f64);
                if self.rand.next_int_bound(10) == 0 {
                    let widen =
                        f(f(self.rand.next_float() as f64 * self.rand.next_float() as f64) * 3.0);
                    thickness = f(thickness * f(widen + 1.0));
                }
                let seed = self.rand.next_long();
                self.add_tunnel(
                    world, seed, origin_x, origin_z, primer, x, y, z, thickness, yaw, pitch, 0, 0,
                    1.0,
                );
            }
        }
    }

    /// `addRoom`：一个球形空洞，等价于 `total = -1`、`heightMod = 0.5` 的隧道。
    #[allow(clippy::too_many_arguments)]
    fn add_room(
        &mut self,
        world: &CarverWorld,
        origin_x: i32,
        origin_z: i32,
        primer: &mut ChunkPrimer,
        x: f64,
        y: f64,
        z: f64,
    ) {
        let seed = self.rand.next_long();
        let thickness = f(1.0 + f(self.rand.next_float() as f64 * 6.0));
        self.add_tunnel(
            world, seed, origin_x, origin_z, primer, x, y, z, thickness, 0.0, 0.0, -1, -1, 0.5,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_tunnel(
        &self,
        world: &CarverWorld,
        seed: i64,
        origin_x: i32,
        origin_z: i32,
        primer: &mut ChunkPrimer,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        thickness: f64,
        mut yaw: f64,
        mut pitch: f64,
        mut step: i32,
        mut total: i32,
        height_mod: f64,
    ) {
        let center_x = (origin_x * 16 + 8) as f64;
        let center_z = (origin_z * 16 + 8) as f64;
        let mut drift_yaw = f(0.0);
        let mut drift_pitch = f(0.0);
        let mut random = JavaRandom::new(seed);

        if total <= 0 {
            let span = self.range * 16 - 16;
            total = span - random.next_int_bound(span / 4);
        }

        let mut single_step = false;
        if step == -1 {
            step = total / 2;
            single_step = true;
        }

        let branch_at = random.next_int_bound(total / 2) + total / 4;
        let branches_more = random.next_int_bound(6) == 0;

        while step < total {
            let radius_xz = 1.5 + f(sin(f(f(f(step as f64) * PI_F) / f(total as f64))) * thickness);
            let radius_y = radius_xz * height_mod;
            let cos_pitch = f(cos(pitch));
            let sin_pitch = f(sin(pitch));

            x += f(cos(yaw) * cos_pitch);
            y += sin_pitch;
            z += f(sin(yaw) * cos_pitch);

            if branches_more {
                pitch = f(pitch * 0.9200000166893005);
            } else {
                pitch = f(pitch * 0.699999988079071);
            }
            pitch = f(pitch + f(drift_yaw * 0.10000000149011612));
            yaw = f(yaw + f(drift_pitch * 0.10000000149011612));

            drift_yaw = f(drift_yaw * 0.8999999761581421);
            drift_pitch = f(drift_pitch * 0.75);

            let r1 = random.next_float() as f64;
            let r2 = random.next_float() as f64;
            let r3 = random.next_float() as f64;
            drift_yaw = f(drift_yaw + f(f(f(r1 - r2) * r3) * 2.0));
            let r4 = random.next_float() as f64;
            let r5 = random.next_float() as f64;
            let r6 = random.next_float() as f64;
            drift_pitch = f(drift_pitch + f(f(f(r4 - r5) * r6) * 4.0));

            if !single_step && step == branch_at && thickness > 1.0 && total > 0 {
                let yaw_a = f(yaw - f(PI_F / 2.0));
                let pitch_a = f(pitch / 3.0);
                let seed_a = random.next_long();
                let thickness_a = f(f(random.next_float() as f64 * 0.5) + 0.5);
                self.add_tunnel(
                    world,
                    seed_a,
                    origin_x,
                    origin_z,
                    primer,
                    x,
                    y,
                    z,
                    thickness_a,
                    yaw_a,
                    pitch_a,
                    step,
                    total,
                    1.0,
                );

                let yaw_b = f(yaw + f(PI_F / 2.0));
                let seed_b = random.next_long();
                let thickness_b = f(f(random.next_float() as f64 * 0.5) + 0.5);
                self.add_tunnel(
                    world,
                    seed_b,
                    origin_x,
                    origin_z,
                    primer,
                    x,
                    y,
                    z,
                    thickness_b,
                    yaw_b,
                    pitch_a,
                    step,
                    total,
                    1.0,
                );
                return;
            }

            if single_step || random.next_int_bound(4) != 0 {
                let dx = x - center_x;
                let dz = z - center_z;
                let remaining = (total - step) as f64;
                let reach = f(f(thickness + 2.0) + 16.0);
                if dx * dx + dz * dz - remaining * remaining > reach * reach {
                    return;
                }

                if x >= (center_x - 16.0) - radius_xz * 2.0
                    && z >= (center_z - 16.0) - radius_xz * 2.0
                    && x <= (center_x + 16.0) + radius_xz * 2.0
                    && z <= (center_z + 16.0) + radius_xz * 2.0
                {
                    self.carve(
                        world, primer, origin_x, origin_z, x, y, z, radius_xz, radius_y,
                    );
                }
            }

            if single_step {
                break;
            }
            step += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn carve(
        &self,
        world: &CarverWorld,
        primer: &mut ChunkPrimer,
        origin_x: i32,
        origin_z: i32,
        center_x: f64,
        center_y: f64,
        center_z: f64,
        radius_xz: f64,
        radius_y: f64,
    ) {
        let mut min_x = (floor(center_x - radius_xz) - origin_x * 16) - 1;
        let mut max_x = (floor(center_x + radius_xz) - origin_x * 16) + 1;
        let mut min_y = floor(center_y - radius_y) - 1;
        let mut max_y = floor(center_y + radius_y) + 1;
        let mut min_z = (floor(center_z - radius_xz) - origin_z * 16) - 1;
        let mut max_z = (floor(center_z + radius_xz) - origin_z * 16) + 1;

        if min_x < 0 {
            min_x = 0;
        }
        if max_x > 16 {
            max_x = 16;
        }
        if min_y < 1 {
            min_y = 1;
        }
        if max_y > 248 {
            max_y = 248;
        }
        if min_z < 0 {
            min_z = 0;
        }
        if max_z > 16 {
            max_z = 16;
        }

        // 先沿管道外壳扫一遍：碰到水或岩浆就整段放弃，免得挖穿海底
        let mut wet = false;
        'outer: for x in min_x..max_x {
            for z in min_z..max_z {
                let mut y = max_y + 1;
                while y >= min_y - 1 {
                    if (0..256).contains(&y) {
                        let id = primer.get_block_state(x, y, z) >> 4;
                        if id == 8 || id == 9 {
                            wet = true;
                        }
                        if y != min_y - 1
                            && x != min_x
                            && x != max_x - 1
                            && z != min_z
                            && z != max_z - 1
                        {
                            y = min_y;
                        }
                    }
                    y -= 1;
                }
                if wet {
                    break 'outer;
                }
            }
        }

        if wet {
            return;
        }

        for x in min_x..max_x {
            let norm_x = ((x + origin_x * 16) as f64 + 0.5 - center_x) / radius_xz;
            for z in min_z..max_z {
                let norm_z = ((z + origin_z * 16) as f64 + 0.5 - center_z) / radius_xz;
                if norm_x * norm_x + norm_z * norm_z >= 1.0 {
                    continue;
                }

                let mut grass_above = false;
                let mut y = max_y;
                while y > min_y {
                    let norm_y = ((y - 1) as f64 + 0.5 - center_y) / radius_y;
                    if norm_y > -0.7 && norm_x * norm_x + norm_y * norm_y + norm_z * norm_z < 1.0 {
                        let above = primer.get_block_state(x, y + 1, z) >> 4;
                        let here = primer.get_block_state(x, y, z) >> 4;
                        if here == 2 || here == 110 {
                            grass_above = true;
                        }
                        if can_replace(here, above) {
                            if y - 1 < 10 {
                                primer.set_block_state(x, y, z, BLK_LAVA);
                            } else {
                                primer.set_block_state(x, y, z, 0);
                                if grass_above && (primer.get_block_state(x, y - 1, z) >> 4) == 3 {
                                    let top = world.top_block(x + origin_x * 16, z + origin_z * 16);
                                    primer.set_block_state(x, y - 1, z, (top >> 4) << 4);
                                }
                            }
                        }
                    }
                    y -= 1;
                }
            }
        }
    }
}

/// `canReplaceBlock`：能被洞穴挖掉的方块。
fn can_replace(here: i32, above: i32) -> bool {
    match here {
        1 | 3 | 2 | 172 | 159 | 24 | 179 | 110 | 78 => true,
        // 沙子和沙砾：上方是水/岩浆时保留，避免把海底挖出空洞
        12 | 13 => above != 8 && above != 9,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaceable_blocks() {
        assert!(can_replace(1, 0));
        assert!(can_replace(78, 0));
        assert!(can_replace(12, 1));
        assert!(!can_replace(12, 8));
        assert!(!can_replace(12, 9));
        assert!(!can_replace(8, 0));
        assert!(!can_replace(0, 0));
    }
}
