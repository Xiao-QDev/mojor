//! 用存档导出的 heightmap CSV（wx,wz,surface_y,block_id）检验 biome 链的海陆对齐：
//! gl_csvcheck <csv> <seed> [对照种子数]
use mojor::level::generator::genlayer::BiomeGen;
use std::collections::HashMap;

fn score(cols: &[(i32, i32, i32, i32)], seed: i64) -> (f64, f64, f64) {
    let g = BiomeGen::new(seed);
    let mut cache: HashMap<(i32, i32), Vec<i32>> = HashMap::new();
    let (mut ww, mut wl, mut lw, mut ll) = (0f64, 0f64, 0f64, 0f64);
    for &(x, z, _, b) in cols {
        let v = cache
            .entry((x >> 4, z >> 4))
            .or_insert_with(|| g.blocks((x >> 4) * 16, (z >> 4) * 16, 16, 16));
        let bio = v[((x & 15) + (z & 15) * 16) as usize];
        let pred_water = matches!(bio, 0 | 24 | 10 | 7 | 11);
        let real_water = b == 8 || b == 9;
        match (pred_water, real_water) {
            (true, true) => ww += 1.0,
            (true, false) => wl += 1.0,
            (false, true) => lw += 1.0,
            (false, false) => ll += 1.0,
        }
    }
    let n = ww + wl + lw + ll;
    let acc = (ww + ll) / n;
    // phi 系数（海陆二值相关）
    let phi = (ww * ll - wl * lw) / ((ww + wl) * (lw + ll) * (ww + lw) * (wl + ll)).sqrt();
    (acc, phi, (ww + wl) / n)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut cols = Vec::new();
    for l in std::fs::read_to_string(&a[0]).unwrap().lines().skip(1) {
        let v: Vec<i32> = l.split(',').map(|s| s.trim().parse().unwrap()).collect();
        cols.push((v[0], v[1], v[2], v[3]));
    }
    let seed: i64 = a[1].parse().unwrap();
    let nnull: u64 = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(0);
    let (acc, phi, pw) = score(&cols, seed);
    println!("seed {seed}: acc {acc:.4} phi {phi:+.4} 预测水域 {pw:.3}");
    let mut s = 0x9E3779B97F4A7C15u64;
    for _ in 0..nnull {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
        let rs = (s >> 32) as u32 as i64;
        let (acc, phi, pw) = score(&cols, rs);
        println!("  对照 {rs}: acc {acc:.4} phi {phi:+.4} 预测水域 {pw:.3}");
    }
}
