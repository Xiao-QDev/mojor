//! 深度优先导出 biome 层树每个节点的输出（与原版对拍）：gl_tree <seed> <x> <z> <w> <h>
use mojor::level::generator::genlayer::BiomeGen;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let g = BiomeGen::new(a[0]);
    for (p, n, v) in g.debug_tree(a[1] as i32, a[2] as i32, a[3] as i32, a[4] as i32) {
        println!("{p} {n} {}", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" "));
    }
}
