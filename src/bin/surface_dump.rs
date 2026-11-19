//! 导出地表阶段前后的区块，供与原版 buildSurfaceAt 对拍：surface_dump <seed> <cx> <cz> <outdir>
use mojor::level::generator::terrain::TerrainGenerator;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut g = TerrainGenerator::new(a[0].parse::<i64>().unwrap() as u32);
    let (pre, post, meta, bio, noise, cave) = g.surface_stages(a[1].parse().unwrap(), a[2].parse().unwrap());
    let d = std::path::Path::new(&a[3]);
    std::fs::write(d.join("pre.bin"), pre).unwrap();
    std::fs::write(d.join("post.bin"), post).unwrap();
    std::fs::write(d.join("meta.bin"), meta).unwrap();
    std::fs::write(d.join("bio.bin"), bio).unwrap();
    std::fs::write(d.join("cave.bin"), cave).unwrap();
    let nb: Vec<u8> = noise.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(d.join("noise.bin"), nb).unwrap();
}
