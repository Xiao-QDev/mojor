//! 打印 buildSurfaces 的地表噪声（16×16，[z*16+x]）：surf_dump <seed> <cx> <cz>
use mojor::level::generator::terrain::TerrainGenerator;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let g = TerrainGenerator::new(a[0] as u32);
    let v = g.surface_noise(a[1] as i32, a[2] as i32);
    println!("{}", v.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(" "));
}
