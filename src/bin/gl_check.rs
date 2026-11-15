//! 打印指定 chunk 的 biome 图，用于和 Python genlayer17 对比
use mojor::level::generator::genlayer::BiomeGen;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: i64 = args.next().unwrap_or_else(|| "57536720".into()).parse().unwrap();
    let cx: i32 = args.next().unwrap_or_else(|| "0".into()).parse().unwrap();
    let cz: i32 = args.next().unwrap_or_else(|| "0".into()).parse().unwrap();

    let g = BiomeGen::new(seed);
    let b = g.chunk_biomes(cx, cz);
    for z in 0..16 {
        for x in 0..16 {
            print!("{:4}", b[x + z * 16]);
        }
        println!();
    }
}
