"""在模拟器里构造 RandomLevelSource 的噪声成员并调用真实 getHeights。
用法: heights.py <seed> <cx> <cz>，stdin 100 个 biome id（[x*10+z]）"""
import emu, sys, struct, os
seed, cx, cz = int(sys.argv[1]) & 0xffffffff, int(sys.argv[2]), int(sys.argv[3])
ids = list(map(int, sys.stdin.read().split()))
e = emu.Emu(); e.run_init_array()
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
# Random（与 ctor 内联初始化一致）
rnd = e.alloc(0xa00)
e.w32(rnd, seed); mt = [seed]
for i in range(1, 624):
    mt.append((0x6c078965 * (mt[-1] ^ (mt[-1] >> 30)) + i) & 0xffffffff)
for i, v in enumerate(mt): e.w32(rnd + 4 + 4 * i, v)
e.w32(rnd + 0x9c4, 624)
this = e.alloc(0x400)
for l in open(os.path.join(os.path.dirname(__file__), 'kernel.txt')):
    o, v = l.split(); e.w32(this + int(o, 16), int(v, 16))
perlin = e.sym('_ZN11PerlinNoiseC1ER6Randomii')
simplex = e.sym('_ZN18PerlinSimplexNoiseC1ER6Randomi')
for off, kind, n in [(0x20,'p',16),(0x24,'p',16),(0x28,'p',8),(0x2c,'s',4),(0x30,'p',10),(0x34,'p',16),(0x38,'p',8)]:
    obj = e.alloc(0x400)
    if kind == 'p': e.call(perlin, obj, rnd, n, 0)
    else: e.call(simplex, obj, rnd, n)
    e.w32(this + off, obj)
gb = e.sym('_ZN5Biome8getBiomeEiPS_')
bios = e.alloc(4 * 100)
for i, b in enumerate(ids): e.w32(bios + 4 * i, e.call(gb, b, 0))
out = e.alloc(4 * 425 + 64)
chunk = e.alloc(0x100)
e.call(e.sym('_ZN17RandomLevelSource10getHeightsEP10LevelChunkPfPP5Biomeiii'), this, chunk, out, bios, cx * 4, 0, cz * 4)
v = e.rf(out, 425)
for c in range(25):
    print(' '.join(repr(x) for x in v[c * 17:(c + 1) * 17]))
if e.unknown - {'_ZNSs4_Rep10_M_disposeERKSaIcE'}: print('unknown', sorted(e.unknown), file=sys.stderr)
