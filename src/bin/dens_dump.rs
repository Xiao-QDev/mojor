//! 打印 5×5×17 密度：dens_dump <seed> <cx> <cz>，stdin 读 100 个 biome id（[x + z*10]，与 getInts 一致）
use mojor::level::generator::terrain::TerrainGenerator;
use std::io::Read;

fn main() {
    let a: Vec<i64> = std::env::args().skip(1).map(|s| s.parse().unwrap()).collect();
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let ids: Vec<u8> = s.split_whitespace().map(|t| t.parse().unwrap()).collect();
    let mut bio = [[0u8; 10]; 10];
    for i in 0..10 {
        for j in 0..10 {
            bio[i][j] = ids[i + j * 10];
        }
    }
    let g = TerrainGenerator::new(a[0] as u32);
    let d = g.density(a[1] as i32, a[2] as i32, &bio);
    for c in d.iter() {
        for v in c.iter() {
            print!("{v:?} ");
        }
        println!();
    }
}
