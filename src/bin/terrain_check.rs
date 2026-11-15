//! 生成一个区块，输出耗时与 biome 分布，用于冒烟验证
use std::collections::HashMap;

use mojor::level::generator::Generator;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u32 = args.next().unwrap_or_else(|| "57536720".into()).parse().unwrap();
    let cx: i32 = args.next().unwrap_or_else(|| "0".into()).parse().unwrap();
    let cz: i32 = args.next().unwrap_or_else(|| "0".into()).parse().unwrap();

    let mut g = Generator::terrain(seed);
    let t0 = std::time::Instant::now();
    let chunk = g.generate(cx, cz);
    let dt = t0.elapsed();

    let mut cnt: HashMap<u8, u32> = HashMap::new();
    for z in 0..16 {
        for x in 0..16 {
            *cnt.entry(chunk.biome_id(x, z)).or_insert(0) += 1;
        }
    }
    let mut v: Vec<_> = cnt.into_iter().collect();
    v.sort_by_key(|&(k, _)| k);
    println!("chunk({cx},{cz}) generate {:?}  biomes={:?}", dt, v);
    for z in 0..16 {
        for x in 0..16 {
            print!("{:4}", chunk.biome_id(x, z));
        }
        println!();
    }
}
