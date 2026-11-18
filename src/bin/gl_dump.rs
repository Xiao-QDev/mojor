//! 打印 biome 层原始输出：gl_dump <seed> <0=1:4 主层|1=voronoi> <x> <z> <w> <h>
//! 输出格式与 JE 1.7.10 GenLayer 对照程序一致（每行 w 个，空格分隔）
use mojor::level::generator::genlayer::BiomeGen;
use std::fmt::Write;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let g = BiomeGen::new(a[0]);
    let (x, z, w, h) = (a[2] as i32, a[3] as i32, a[4] as i32, a[5] as i32);
    let v = if a[1] == 0 { g.raw(x, z, w, h) } else { g.blocks(x, z, w, h) };
    let mut s = String::new();
    for (i, b) in v.iter().enumerate() {
        write!(s, "{b}{}", if i as i32 % w == w - 1 { '\n' } else { ' ' }).unwrap();
    }
    print!("{s}");
}
