"""用原版 buildSurfaceAt 处理 Rust 的地表前区块，与 Rust 地表后结果逐方块比较。
用法: surfcheck.py <seed> <cx> <cz> <dir>"""
import emu, fakeblocks, sys, struct, collections
seed, cx, cz, d = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
pre = open(d+'/pre.bin','rb').read(); post = open(d+'/post.bin','rb').read(); meta = open(d+'/meta.bin','rb').read()
bio = open(d+'/bio.bin','rb').read(); noise = struct.unpack('<256f', open(d+'/noise.bin','rb').read())
e = emu.Emu(); e.run_init_array(); fakeblocks.install(e)
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
# Block::mBlocks[256]：带材质的假方块（材质类型 0 = 空气）
mb = e.sym('_ZN5Block7mBlocksE')
mat_air = e.alloc(16); e.w32(mat_air, 0)
mat_solid = e.alloc(16); e.w32(mat_solid, 7)
for i in range(256):
    b = e.alloc(0x80); e.uc.mem_write(b+4, bytes([i])); e.w32(b+0x54, mat_air if i == 0 else mat_solid)
    e.w32(mb+4*i, b)
chunk = e.alloc(0x10000)
e.uc.mem_write(chunk+0xa3c, pre)
s = (cx*0x14609048 + cz*0x7ebe2d5) & 0xffffffff
rnd = e.alloc(0xa00); e.w32(rnd, s); mt=[s]
for i in range(1,624): mt.append((0x6c078965*(mt[-1]^(mt[-1]>>30))+i)&0xffffffff)
for i,v in enumerate(mt): e.w32(rnd+4+4*i, v)
e.w32(rnd+0x9c4, 624)
gb = e.sym('_ZN5Biome8getBiomeEiPS_')
pos = e.alloc(16)
fb = lambda v: struct.unpack('<I', struct.pack('<f', v))[0]
for x in range(16):
    for z in range(16):
        b = e.call(gb, bio[x+z*16], 0)
        e.w32(pos, cx*16+x); e.w32(pos+4, 0); e.w32(pos+8, cz*16+z)
        e.call(e.r32(e.r32(b)+0x3c), b, rnd, chunk, pos, fb(noise[z*16+x]))
got = bytes(e.uc.mem_read(chunk+0xa3c, 32768))
nib = bytes(e.uc.mem_read(chunk+0x8a3c, 16384))
gm = bytes((nib[i>>1] >> (4*(i&1))) & 15 for i in range(32768))
diff = [i for i in range(32768) if got[i] != post[i]]
mdiff = [i for i in range(32768) if gm[i] != meta[i]]
kinds = collections.Counter((post[i], got[i]) for i in diff)
print(f'seed {seed} chunk {cx},{cz}: block diffs {len(diff)}  meta diffs {len(mdiff)}  biomes {sorted(set(bio))}  (rust,orig): {kinds.most_common(6)}')
cols=sorted(set((i>>11, (i>>7)&15) for i in diff))
print('cols', cols)
sw=e.call(gb,6,0); o=e.r32(sw+0x98)
for x,z in cols:
    v=e.call_f(e.sym('_ZNK18PerlinSimplexNoise8getValueEff'), o, fb((cx*16+x)*0.25), fb((cz*16+z)*0.25))
    print(x,z,'orig v', v, [ (y,post[(x<<11)|(z<<7)|y], got[(x<<11)|(z<<7)|y]) for y in range(59,65)])
