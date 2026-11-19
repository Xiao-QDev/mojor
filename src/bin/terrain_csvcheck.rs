//! 端到端：用 vanilla 地形生成区块，与存档导出的 heightmap CSV 逐列比较地表高度。
//! terrain_csvcheck <csv> <seed>
//! 只比较存档里地表是“地面/水面”的列（排除树叶、草、藤等被装饰覆盖的列）。
use mojor::level::format::chunk::Chunk;
use mojor::level::generator::terrain::TerrainGenerator;
use mojor::level::generator::world::GenWorld;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let seed: u32 = a[1].parse::<i64>().unwrap() as u32;
    let mut cols = HashMap::new();
    for l in std::fs::read_to_string(&a[0]).unwrap().lines().skip(1) {
        let v: Vec<i32> = l.split(',').map(|s| s.trim().parse().unwrap()).collect();
        cols.insert((v[0], v[1]), (v[2], v[3]));
    }
    let mut chunk_keys: Vec<(i32, i32)> = cols.keys().map(|&(x, z)| (x >> 4, z >> 4)).collect();
    chunk_keys.sort();
    chunk_keys.dedup();

    let mut g = TerrainGenerator::new(seed);
    let mut dump = String::new();
    let biomes = mojor::level::generator::genlayer::BiomeGen::new(seed as i64);
    let ground = |b: i32| matches!(b, 1 | 2 | 3 | 12 | 13 | 24 | 8 | 9 | 79 | 80 | 82 | 110 | 172 | 159 | 179);
    let (mut n, mut eq, mut le1, mut le2, mut le4, mut se, mut sb) = (0u64, 0u64, 0u64, 0u64, 0u64, 0f64, 0f64);
    let (mut wn, mut wagree) = (0u64, 0u64);
    // POPULATE=1：先生成全部区块（含一圈边距）的地形，再逐区块跑原版装饰（已移植部分）
    let populate = std::env::var("POPULATE").is_ok();
    let mut all: HashMap<(i32, i32), Chunk> = HashMap::new();
    if populate {
        let mut keys: Vec<(i32, i32)> = chunk_keys
            .iter()
            .flat_map(|&(cx, cz)| (-1..=1).flat_map(move |dx| (-1..=1).map(move |dz| (cx + dx, cz + dz))))
            .collect();
        keys.sort();
        keys.dedup();
        for &(cx, cz) in &keys {
            all.insert((cx, cz), Chunk::empty(cx, cz));
            let mut world = GenWorld::new(&mut all);
            g.generate_chunk(&mut world, cx, cz);
        }
        let mut deco = mojor::level::generator::vanilla::decorator::DecoState::new();
        let mut world = GenWorld::new(&mut all);
        for &(cx, cz) in &chunk_keys {
            mojor::level::generator::vanilla::decorate::post_process(&mut world, cx, cz, seed, false, &mut deco);
        }
    }
    for &(cx, cz) in &chunk_keys {
        let mut chunks = HashMap::new();
        if populate {
            chunks.insert((cx, cz), all[&(cx, cz)].clone());
        } else {
        chunks.insert((cx, cz), Chunk::empty(cx, cz));
        {
            let mut world = GenWorld::new(&mut chunks);
            g.generate_chunk(&mut world, cx, cz);
        }
        }
        let c = &chunks[&(cx, cz)];
        for x in 0..16usize {
            for z in 0..16usize {
                let wx = cx * 16 + x as i32;
                let wz = cz * 16 + z as i32;
                let Some(&(ry, rb)) = cols.get(&(wx, wz)) else { continue };
                let top0 = (0..128usize).rev().find(|&y| c.block(x, y, z) != 0).unwrap_or(0);
                dump.push_str(&format!("{wx},{wz},{ry},{rb},{top0},{},{}\n", c.block(x, top0, z), biomes.biome_at(wx, wz)));
                if !ground(rb) {
                    continue;
                }
                let top = (0..128usize).rev().find(|&y| c.block(x, y, z) != 0).unwrap_or(0) as i32;
                let gw = matches!(c.block(x, top as usize, z), 8 | 9);
                let rw = rb == 8 || rb == 9;
                wn += 1;
                if gw == rw {
                    wagree += 1;
                }
                let e = (top - ry) as f64;
                n += 1;
                se += e * e;
                sb += e;
                let ae = e.abs();
                if ae == 0.0 { eq += 1; }
                if ae <= 1.0 { le1 += 1; }
                if ae <= 2.0 { le2 += 1; }
                if ae <= 4.0 { le4 += 1; }
            }
        }
    }
    if let Some(p) = a.get(2) { std::fs::write(p, dump).unwrap(); }
    let f = |v: u64| v as f64 * 100.0 / n as f64;
    println!(
        "seed {seed}: 列 {n}  RMSE {:.2}  偏差 {:+.2}  相等 {:.1}%  |e|≤1 {:.1}%  |e|≤2 {:.1}%  |e|≤4 {:.1}%  水陆一致 {:.1}%",
        (se / n as f64).sqrt(), sb / n as f64, f(eq), f(le1), f(le2), f(le4), wagree as f64 * 100.0 / wn as f64
    );
}
