//! 统计一片区域的 biome 分布：gl_hist <seed> <cx0> <cz0> <n_chunks>
use mojor::level::generator::genlayer::BiomeGen;
use std::collections::BTreeMap;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let (seed, cx0, cz0, n) = (a[0], a[1] as i32, a[2] as i32, a[3] as i32);
    let g = BiomeGen::new(seed);
    let mut h: BTreeMap<u8, u64> = BTreeMap::new();
    for cx in cx0..cx0 + n {
        for cz in cz0..cz0 + n {
            for b in g.chunk_biomes(cx, cz).iter() {
                *h.entry(*b).or_default() += 1;
            }
        }
    }
    let tot: u64 = h.values().sum();
    let mut v: Vec<_> = h.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (b, c) in v {
        println!("{b:4} {:6.2}%", c as f64 * 100.0 / tot as f64);
    }
}
