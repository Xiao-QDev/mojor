"""在 Rust 地表后的区块上运行原版 LargeFeature::apply（LargeCaveFeature），与 Rust 洞穴结果比较。
用法: cavecheck.py <seed> <cx> <cz> <dir>   dir 里需 post.bin(地表后) 与 cave.bin(Rust 洞穴后，可选)"""
import emu, fakeblocks, sys, os, collections
seed, cx, cz, d = int(sys.argv[1]) & 0xffffffff, int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
pre = open(d + '/post.bin', 'rb').read()
e = emu.Emu(); e.run_init_array(); fakeblocks.install(e)
e.patch_ret(e.sym('_ZN10LevelChunk17deferLightEmitterERK8BlockPos'))
chunk = e.alloc(0x10000)
e.uc.mem_write(chunk + 0xa3c, pre)
e.w32(chunk + 0x8, cx * 16); e.w32(chunk + 0xc, 0); e.w32(chunk + 0x10, cz * 16)
e.w32(chunk + 0x20, cx); e.w32(chunk + 0x24, cz)
feat = e.alloc(0x100)
e.call(e.sym('_ZN12LargeFeatureC2Ev'), feat)
e.w32(feat, e.sym('_ZTV16LargeCaveFeature') + 8)
bs = e.alloc(0x100); e.w32(bs + 0x30, seed)
rnd = e.alloc(0xa00)
e.call(e.sym('_ZN12LargeFeature5applyEP11ChunkSourceR10LevelChunkP11BiomeSourceR6Random'), feat, 0, chunk, bs, rnd)
got = bytes(e.uc.mem_read(chunk + 0xa3c, 32768))
open(d + '/orig_cave.bin', 'wb').write(got)
carved = sum(a != b for a, b in zip(pre, got))
msg = f'seed {seed} chunk {cx},{cz}: 原版改动 {carved} 个方块'
if os.path.exists(d + '/cave.bin'):
    mine = open(d + '/cave.bin', 'rb').read()
    diff = [i for i in range(32768) if mine[i] != got[i]]
    msg += f'  与 Rust 不同 {len(diff)}  (rust,orig) {collections.Counter((mine[i], got[i]) for i in diff).most_common(5)}'
print(msg, '| unknown', sorted(e.unknown - {"_ZNSs4_Rep10_M_disposeERKSaIcE"})[:5])
