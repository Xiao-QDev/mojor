//! 对拍用：找各 biome（按区块 (15,15) 处）的一个区块。biome_find <seed> <radius_chunks>
use mojor::level::generator::genlayer::BiomeGen;
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let seed = a[0].parse::<i64>().unwrap();
    let r: i32 = a[1].parse().unwrap();
    let g = BiomeGen::new(seed);
    let mut found: BTreeMap<u8, (i32, i32)> = BTreeMap::new();
    let mut step = 0;
    'outer: for ring in 0..r {
        for cx in -ring..=ring {
            for cz in -ring..=ring {
                if cx.abs() != ring && cz.abs() != ring {
                    continue;
                }
                step += 1;
                if step % 7 != 0 {
                    continue;
                }
                let b = g.biome_at(cx * 16 + 15, cz * 16 + 15);
                found.entry(b).or_insert((cx, cz));
                if found.len() > 60 {
                    break 'outer;
                }
            }
        }
    }
    for (b, (cx, cz)) in found {
        println!("{b} {cx} {cz}");
    }
}
