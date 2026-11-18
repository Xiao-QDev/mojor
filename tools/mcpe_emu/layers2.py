"""原版 biome 链对拍：按游戏自身的窗口大小（主层 10×10 / voronoi 16×16）分块请求。
用法: layers2.py <seed> <which> <cx0> <cz0> <n>  （逐区块，输出与 Rust raw_chunk / chunk_biomes 同序）"""
import emu, sys
seed, which, cx0, cz0, n = [int(a) for a in sys.argv[1:6]]
e = emu.Emu()
ok, bad = e.run_init_array(verbose=True)
if bad: raise SystemExit(f'init_array 失败：{bad}/{ok + bad} 个构造函数未完成')
import fakeblocks; fakeblocks.install(e)
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
sp_main = e.alloc(16); sp_zoom = e.alloc(16)
e.call(e.sym('_ZN5Layer16getDefaultLayersExRSt10shared_ptrIS_ES2_13GeneratorType'), seed & 0xffffffff, 0, sp_main, sp_zoom, 1)
layer = e.r32(sp_main if which == 0 else sp_zoom)
bufs = e.alloc(16); b1 = e.alloc(0x10000); b2 = e.alloc(0x10000)
for cx in range(cx0, cx0 + n):
    for cz in range(cz0, cz0 + n):
        e.w32(bufs, b1); e.w32(bufs + 4, b2)
        if which == 0: args = (cx * 4 - 2, cz * 4 - 2, 10, 10)
        else: args = (cx * 16, cz * 16, 16, 16)
        e.call(e.r32(e.r32(layer) + 0xc), layer, bufs, *args)
        out = e.r32(bufs); w, h = args[2], args[3]
        print(cx, cz, ' '.join(str(e.r32(out + 4 * i)) for i in range(w * h)))
