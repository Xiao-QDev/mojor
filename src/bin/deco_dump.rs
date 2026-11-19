//! 对拍用：导出中心区块 decorate 前后的 3×3 区块状态（tools/mcpe_emu/decocheck.py 读）。
//! deco_dump <seed> <cx> <cz> <outdir>
//! 输出：pre.bin / post.bin（每区块 32768 方块 + 32768 data + 256 高度图 + 256 biome，按 dz,dx 行优先）
use mojor::level::format::chunk::Chunk;
use mojor::level::generator::terrain::TerrainGenerator;
use mojor::level::generator::vanilla::decorate::{post_process, DEBUG_STOP};
use mojor::level::generator::vanilla::decorator::DecoState;
use mojor::level::generator::world::GenWorld;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

fn dump(all: &HashMap<(i32, i32), Chunk>, cx: i32, cz: i32) -> Vec<u8> {
    let mut out = Vec::new();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let c = &all[&(cx + dx, cz + dz)];
            let mut b = vec![0u8; 32768];
            let mut m = vec![0u8; 32768];
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..128 {
                        b[x << 11 | z << 7 | y] = c.block(x, y, z);
                        m[x << 11 | z << 7 | y] = c.meta(x, y, z);
                    }
                }
            }
            out.extend(b);
            out.extend(m);
            for z in 0..16 {
                for x in 0..16 {
                    out.push(c.height(x, z));
                }
            }
            for z in 0..16 {
                for x in 0..16 {
                    out.push(c.biome_id(x, z));
                }
            }
        }
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let seed = a[0].parse::<i64>().unwrap() as u32;
    let cx: i32 = a[1].parse().unwrap();
    let cz: i32 = a[2].parse().unwrap();
    let out = std::path::Path::new(&a[3]);
    std::fs::create_dir_all(out).unwrap();
    // 对拍对象是 x86 版：构造参数求值顺序按 x86
    mojor::level::generator::vanilla::features::X86_ORDER.store(std::env::var("ARM").is_err(), Ordering::Relaxed);
    let mut g = TerrainGenerator::new(seed);
    let mut all: HashMap<(i32, i32), Chunk> = HashMap::new();
    for dx in -2..=2 {
        for dz in -2..=2 {
            all.insert((cx + dx, cz + dz), Chunk::empty(cx + dx, cz + dz));
            let mut world = GenWorld::new(&mut all);
            g.generate_chunk(&mut world, cx + dx, cz + dz);
        }
    }
    let mut deco = DecoState::new();
    DEBUG_STOP.store(1, Ordering::Relaxed);
    {
        let mut world = GenWorld::new(&mut all);
        post_process(&mut world, cx, cz, seed, false, &mut deco);
    }
    std::fs::write(out.join("pre.bin"), dump(&all, cx, cz)).unwrap();
    // 重新跑一次：湖与结冰是确定的，第二次在 decorate 之后停
    let mut all2: HashMap<(i32, i32), Chunk> = HashMap::new();
    for dx in -2..=2 {
        for dz in -2..=2 {
            all2.insert((cx + dx, cz + dz), Chunk::empty(cx + dx, cz + dz));
            let mut world = GenWorld::new(&mut all2);
            g.generate_chunk(&mut world, cx + dx, cz + dz);
        }
    }
    // LIQUID=1：跑完整个 postProcess（含 _fixWaterAlongEdges 与液体 tick）
    DEBUG_STOP.store(if std::env::var("LIQUID").is_ok() { 0 } else { 2 }, Ordering::Relaxed);
    {
        let mut world = GenWorld::new(&mut all2);
        post_process(&mut world, cx, cz, seed, false, &mut deco);
    }
    std::fs::write(out.join("post.bin"), dump(&all2, cx, cz)).unwrap();
    let biome = all[&(cx, cz)].biome_id(15, 15);
    println!("biome {biome}");
}
