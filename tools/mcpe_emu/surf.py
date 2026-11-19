import emu, sys
seed, cx, cz = int(sys.argv[1]) & 0xffffffff, int(sys.argv[2]), int(sys.argv[3])
e = emu.Emu(); e.run_init_array()
rnd = e.alloc(0xa00); e.w32(rnd, seed); mt=[seed]
for i in range(1,624): mt.append((0x6c078965*(mt[-1]^(mt[-1]>>30))+i)&0xffffffff)
for i,v in enumerate(mt): e.w32(rnd+4+4*i, v)
e.w32(rnd+0x9c4, 624)
P=e.sym('_ZN11PerlinNoiseC1ER6Randomii')
for n in [16,16,8]: e.call(P, e.alloc(0x40), rnd, n, 0)
ps=e.alloc(0x40); e.call(e.sym('_ZN18PerlinSimplexNoiseC1ER6Randomi'), ps, rnd, 4)
out=e.alloc(4*256); pos=e.alloc(8); e.wf(pos,[cx*16.0, cz*16.0]); sc=e.alloc(8); e.wf(sc,[0.125,0.125])
import struct
fa=struct.unpack('<I',struct.pack('<f',1.0))[0]; fb=struct.unpack('<I',struct.pack('<f',0.5))[0]
e.call(e.sym('_ZNK18PerlinSimplexNoise9getRegionEPfRK4Vec2iiS3_ff'), ps, out, pos, 16, 16, sc, fa, fb)
print(' '.join(repr(v) for v in e.rf(out,256)))
