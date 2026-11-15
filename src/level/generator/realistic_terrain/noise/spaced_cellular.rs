//! 对应 `rtg/noise/SpacedCellularNoise.php`（RTG+，AGPLv3）。
//! 原始 libnoise 由 Jason Bevins 编写，Java 版 Thomas J. Hodge，RTG 改动 mncat77/jtjj222/Zeno410。
//!
//! 这个类有几处**看起来像 bug 但必须保留**的写法，源码里也明确标注了：
//!
//! 1. `area_points` 里标记占用用的是 `used[i]` 而不是 `used[index]`
//! 2. `eval2d` 先用 `next_distance` 做浮点相等比较，再决定要不要扩大搜索
//! 3. 点集缓存按插入顺序淘汰，容量 256

use std::collections::{HashMap, VecDeque};

use crate::level::generator::realistic_terrain::math::JavaRandom;

use super::voronoi::VoronoiResult;

pub struct SpacedCellularNoise {
    points: Vec<[f64; 2]>,
    cache: HashMap<(i32, i32), Vec<[f64; 2]>>,
    order: VecDeque<(i32, i32)>,
}

impl SpacedCellularNoise {
    pub fn new(seed: i64) -> Self {
        let mut xr = JavaRandom::new(seed);
        let y_seed = xr.next_long();
        let mut yr = JavaRandom::new(y_seed);
        xr.set_seed(seed);

        let mut points: Vec<[f64; 2]> = Vec::with_capacity(100);
        while points.len() < 100 {
            let x = xr.next_double();
            let y = yr.next_double();
            let mut distance: f64 = 1.0;
            for p in &points {
                let mut dx = (x - p[0]).abs();
                let mut dy = (y - p[1]).abs();
                if dx > 0.5 {
                    dx = 1.0 - dx;
                }
                if dy > 0.5 {
                    dy = 1.0 - dy;
                }
                distance = distance.min(dx * dx + dy * dy);
            }
            if distance >= 0.005 {
                points.push([x, y]);
            }
        }

        Self {
            points,
            cache: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// `java.awt.Point` 继承的是 `Point2D.hashCode()`：取 double 的位模式，
    /// 不是整数坐标 hash。写错会让整片地形错位。
    fn point_hash(x: i32, y: i32) -> i64 {
        let xb = (x as f64).to_bits() as i64;
        let yb = (y as f64).to_bits() as i64;
        let bits = xb ^ yb.wrapping_mul(31);
        (bits ^ (bits >> 32)) as i32 as i64
    }

    fn area_points(&mut self, x: i32, y: i32) -> Vec<[f64; 2]> {
        if let Some(points) = self.cache.get(&(x, y)) {
            return points.clone();
        }

        let mut random = JavaRandom::new(Self::point_hash(x, y));
        let mut used = [false; 100];
        let mut result: Vec<[f64; 2]> = Vec::with_capacity(25);
        let mut index = 0usize;

        for i in 0..25 {
            let advance = random.next_int_bound(100);
            for _ in 0..advance {
                while used[index] {
                    index += 1;
                    if index >= 100 {
                        index = 0;
                    }
                }
            }
            result.push([
                self.points[index][0] + x as f64,
                self.points[index][1] + y as f64,
            ]);
            // 原实现就是 used[i]，不是 used[index]
            used[i] = true;
        }

        if self.cache.len() >= 256 {
            if let Some(key) = self.order.pop_front() {
                self.cache.remove(&key);
            }
        }
        self.order.push_back((x, y));
        self.cache.insert((x, y), result.clone());
        result
    }

    pub fn eval2d(&mut self, x: f64, y: f64) -> VoronoiResult {
        let x = x / 5.0;
        let y = y / 5.0;
        let xi = if x > 0.0 { x as i32 } else { x as i32 - 1 };
        let yi = if y > 0.0 { y as i32 } else { y as i32 - 1 };

        let mut result = VoronoiResult::new();
        let points = self.area_points(xi, yi);
        result.evaluate(&points, x, y);

        let fx = x - xi as f64;
        let fy = y - yi as f64;
        let neighbors = [
            (0, -1, fy),
            (-1, 0, fx),
            (0, 1, 1.0 - fy),
            (1, 0, 1.0 - fx),
            (-1, -1, fy.min(fx)),
            (-1, 1, (1.0 - fy).min(fx)),
            (1, 1, (1.0 - fy).min(1.0 - fx)),
            (1, -1, fy.min(1.0 - fx)),
        ];
        for (dx, dz, distance) in neighbors {
            if distance != result.next() {
                let points = self.area_points(xi + dx, yi + dz);
                result.evaluate(&points, x, y);
            }
        }
        result
    }
}
