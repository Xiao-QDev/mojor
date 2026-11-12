// 对齐 Nukkit 的 NukkitRandom 世界生成专用随机数
// 种子先过一遍 CRC32 再进入线性同余 序列与 Java 版逐位一致
#[derive(Clone)]
pub struct NukkitRandom {
    seed: i64,
}

impl NukkitRandom {
    pub fn new(seed: i64) -> Self {
        let mut random = Self { seed: 0 };
        random.set_seed(seed);
        random
    }

    pub fn set_seed(&mut self, seeds: i64) {
        self.seed = crc32(&(seeds as i32).to_be_bytes()) as i64;
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    // Java 里是 int 运算 溢出回绕 位移用算术右移
    pub fn next_signed_int(&mut self) -> i32 {
        let mixed = self.seed.wrapping_mul(65535).wrapping_add(31337) as i32;
        let t = (mixed >> 8).wrapping_add(1337);
        // t 提升为 long 时符号扩展 与 Java 的 seed ^= t 一致
        self.seed ^= t as i64;
        t
    }

    pub fn next_int(&mut self) -> i32 {
        self.next_signed_int() & 0x7fff_ffff
    }

    pub fn next_double(&mut self) -> f64 {
        self.next_int() as f64 / 0x7fff_ffff as f64
    }

    // Java 先把 int 转成 float 再除 所以这里也走 f32
    pub fn next_float(&mut self) -> f32 {
        self.next_int() as f32 / 0x7fff_ffff as f32
    }

    pub fn next_boolean(&mut self) -> bool {
        (self.next_signed_int() & 0x01) == 0
    }

    pub fn next_range(&mut self, start: i32, end: i32) -> i32 {
        start.wrapping_add(self.next_int() % (end + 1 - start))
    }

    pub fn next_bounded_int(&mut self, bound: i32) -> i32 {
        self.next_int() % bound
    }
}

// 标准 IEEE CRC32 与 java.util.zip.CRC32 结果相同
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_reference_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = NukkitRandom::new(1337);
        let mut b = NukkitRandom::new(1337);
        for _ in 0..64 {
            assert_eq!(a.next_signed_int(), b.next_signed_int());
        }
    }

    #[test]
    fn next_int_is_non_negative() {
        let mut random = NukkitRandom::new(-12345);
        for _ in 0..256 {
            assert!(random.next_int() >= 0);
        }
    }

    // 有界取值不能越界 且覆盖到区间两端
    #[test]
    fn bounded_int_stays_in_range() {
        let mut random = NukkitRandom::new(7);
        let mut seen_zero = false;
        for _ in 0..512 {
            let v = random.next_bounded_int(4);
            assert!((0..4).contains(&v), "越界 {v}");
            seen_zero |= v == 0;
        }
        assert!(seen_zero);
    }

    #[test]
    fn range_is_inclusive() {
        let mut random = NukkitRandom::new(9);
        for _ in 0..512 {
            let v = random.next_range(1, 5);
            assert!((1..=5).contains(&v), "越界 {v}");
        }
    }
}
