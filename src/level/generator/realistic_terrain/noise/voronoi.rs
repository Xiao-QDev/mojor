//! 对应 `rtg/noise/VoronoiResult.php`（Zeno410，AGPLv3）。
//! 距离全部用**平方**比较，与源码一致，不要提前开方。

const UNREACHABLE: f64 = 32_000_000.0;

#[derive(Clone, Debug)]
pub struct VoronoiResult {
    shortest: f64,
    next: f64,
    pub x: f64,
    pub z: f64,
}

impl Default for VoronoiResult {
    fn default() -> Self {
        Self {
            shortest: UNREACHABLE,
            next: UNREACHABLE,
            x: UNREACHABLE,
            z: UNREACHABLE,
        }
    }
}

impl VoronoiResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shortest(&self) -> f64 {
        self.shortest
    }

    pub fn next(&self) -> f64 {
        self.next
    }

    pub fn border_value(&self) -> f64 {
        self.shortest / self.next
    }

    pub fn interior_value(&self) -> f64 {
        (self.next - self.shortest) / self.next
    }

    /// 把最近点沿方向推到给定半径处；退化时返回 NaN。
    pub fn to_length(&self, x: f64, z: f64, radius: f64) -> [f64; 2] {
        let dx = x - self.x;
        let dz = z - self.z;
        let d = (dx * dx + dz * dz).sqrt();
        if d == 0.0 {
            return [f64::NAN, f64::NAN];
        }
        [self.x + dx * (radius / d), self.z + dz * (radius / d)]
    }

    pub fn evaluate(&mut self, points: &[[f64; 2]], x: f64, z: f64) {
        let mut shortest = self.shortest;
        let mut next = self.next;
        let mut closest_x = self.x;
        let mut closest_z = self.z;
        for p in points {
            let dx = p[0] - x;
            let dz = p[1] - z;
            let d = dx * dx + dz * dz;
            if d < shortest {
                next = shortest;
                shortest = d;
                closest_x = p[0];
                closest_z = p[1];
            } else if d < next {
                next = d;
            }
        }
        self.shortest = shortest;
        self.next = next;
        self.x = closest_x;
        self.z = closest_z;
    }
}
