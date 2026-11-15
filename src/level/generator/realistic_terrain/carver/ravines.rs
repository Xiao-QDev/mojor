//! 对应 `rtg/carver/MapGenRavine.php` + `MapGenRavineRTG.php`。
//!
//! 峡谷比洞穴更「扁」：垂直半径额外除以 6，另外每层有一个随机缩放表
//! （`$rs`），让裂谷在水平方向收窄。触发概率由 `ravineChance` 控制
//! （vanilla 默认 120，约每 120 个 origin 一条）。

use crate::level::generator::realistic_terrain::carver::CarverWorld;
use crate::level::generator::realistic_terrain::carver::math_helper::{cos, floor, sin};
use crate::level::generator::realistic_terrain::chunk_primer::ChunkPrimer;
use crate::level::generator::realistic_terrain::math::{JavaRandom, f};

/// `id 10 << 4`：峡谷底部灌岩浆
const BLK_FLOWING_LAVA: i32 = 160;

const PI_F: f64 = std::f32::consts::PI as f64;

pub struct Ravines {
    /// `MapGenBase::$range`，固定 8
    pub range: i32,
    rand: JavaRandom,
    ravine_chance: i32,
}

impl Ravines {
    pub fn new(ravine_chance: i32) -> Self {
        assert!(
            ravine_chance >= 1,
            "ravineChance 必须 >= 1（PHP 里 nextInt(0) 会崩）"
        );
        Self {
            range: 8,
            rand: JavaRandom::new(0),
            ravine_chance,
        }
    }

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

    /// `MapGenRavineRTG::recursiveGenerate`
    fn recursive_generate(
        &mut self,
        world: &CarverWorld,
        chunk_x: i32,
        chunk_z: i32,
        origin_x: i32,
        origin_z: i32,
        primer: &mut ChunkPrimer,
    ) {
        if self.rand.next_int_bound(self.ravine_chance) != 0 {
            return;
        }

        let x = (chunk_x * 16 + self.rand.next_int_bound(16)) as f64;
        let depth_bound = self.rand.next_int_bound(40) + 8;
        let y = (self.rand.next_int_bound(depth_bound) + 20) as f64;
        let z = (chunk_z * 16 + self.rand.next_int_bound(16)) as f64;

        let yaw = f(self.rand.next_float() as f64 * f(PI_F * 2.0));
        let pitch = f(f((self.rand.next_float() as f64 - 0.5) * 2.0) / 8.0);
        let thickness =
            f(f(self.rand.next_float() as f64 * 2.0 + self.rand.next_float() as f64) * 2.0);
        let seed = self.rand.next_long();
        self.add_tunnel(
            world, seed, origin_x, origin_z, primer, x, y, z, thickness, yaw, pitch, 0, 0, 3.0,
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

        // 每层的水平收窄系数
        let mut scale = f(1.0);
        let mut rs = [0.0_f64; 256];
        for (index, slot) in rs.iter_mut().enumerate() {
            if index == 0 || random.next_int_bound(3) == 0 {
                scale = f(1.0 + f(random.next_float() as f64 * random.next_float() as f64));
            }
            *slot = f(scale * scale);
        }

        let mut drift_yaw = f(0.0);
        let mut drift_pitch = f(0.0);

        while step < total {
            let radius_xz = 1.5 + f(sin(f(f(f(step as f64) * PI_F) / f(total as f64))) * thickness);
            let mut radius_y = radius_xz * height_mod;
            let narrow = radius_xz * (random.next_float() as f64 * 0.25 + 0.75);
            radius_y *= random.next_float() as f64 * 0.25 + 0.75;

            let cos_pitch = f(cos(pitch));
            let sin_pitch = f(sin(pitch));

            x += f(cos(yaw) * cos_pitch);
            y += sin_pitch;
            z += f(sin(yaw) * cos_pitch);

            pitch = f(pitch * 0.699999988079071);
            pitch = f(pitch + f(drift_yaw * 0.05000000074505806));
            yaw = f(yaw + f(drift_pitch * 0.05000000074505806));

            drift_yaw = f(drift_yaw * 0.800000011920929);
            drift_pitch = f(drift_pitch * 0.5);

            let r1 = random.next_float() as f64;
            let r2 = random.next_float() as f64;
            let r3 = random.next_float() as f64;
            drift_yaw = f(drift_yaw + f(f(f(r1 - r2) * r3) * 2.0));
            let r4 = random.next_float() as f64;
            let r5 = random.next_float() as f64;
            let r6 = random.next_float() as f64;
            drift_pitch = f(drift_pitch + f(f(f(r4 - r5) * r6) * 4.0));

            if single_step || random.next_int_bound(4) != 0 {
                let dx = x - center_x;
                let dz = z - center_z;
                let remaining = (total - step) as f64;
                let reach = f(f(thickness + 2.0) + 16.0);
                if dx * dx + dz * dz - remaining * remaining > reach * reach {
                    return;
                }

                if x >= (center_x - 16.0) - narrow * 2.0
                    && z >= (center_z - 16.0) - narrow * 2.0
                    && x <= (center_x + 16.0) + narrow * 2.0
                    && z <= (center_z + 16.0) + narrow * 2.0
                {
                    self.carve(
                        world, primer, origin_x, origin_z, x, y, z, narrow, radius_y, &rs,
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
        rs: &[f64; 256],
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
                    let horizontal = norm_x * norm_x + norm_z * norm_z;
                    if horizontal * rs[(y - 1) as usize] + (norm_y * norm_y) / 6.0 < 1.0 {
                        let here = primer.get_block_state(x, y, z) >> 4;
                        if here == 2 {
                            grass_above = true;
                        }
                        if here == 1 || here == 3 || here == 2 {
                            if y - 1 < 10 {
                                primer.set_block_state(x, y, z, BLK_FLOWING_LAVA);
                            } else {
                                primer.set_block_state(x, y, z, 0);
                                if grass_above && (primer.get_block_state(x, y - 1, z) >> 4) == 3 {
                                    // 注意峡谷这里没有 `>> 4 << 4`，与洞穴不同
                                    let top = world.top_block(x + origin_x * 16, z + origin_z * 16);
                                    primer.set_block_state(x, y - 1, z, top);
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
