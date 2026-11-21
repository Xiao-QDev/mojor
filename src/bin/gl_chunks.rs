//! 逐区块输出 biome（与 layers2.py 同格式）：gl_chunks <seed> <0=raw 10x10|1=block 16x16> <cx0> <cz0> <n>
use mojor::level::generator::genlayer::BiomeGen;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let g = BiomeGen::new(a[0]);
    let (which, cx0, cz0, n) = (a[1], a[2] as i32, a[3] as i32, a[4] as i32);
    for cx in cx0..cx0 + n {
        for cz in cz0..cz0 + n {
            let v: Vec<i32> = if which == 0 {
                g.raw(cx * 4 - 2, cz * 4 - 2, 10, 10)
            } else {
                g.blocks(cx * 16, cz * 16, 16, 16)
            };
            println!("{cx} {cz} {}", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "));
        }
    }
}
