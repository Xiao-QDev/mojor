"""深度优先遍历原版 layer 树，每个节点在同一小窗口上取 getInts。用法: tree2.py <seed> <x> <z> <w> <h>"""
import emu, fakeblocks, subprocess, sys
seed, X, Z, W, H = [int(a) for a in sys.argv[1:6]]
e = emu.Emu(); e.run_init_array(); fakeblocks.install(e)
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs')); e.call(e.sym('_ZN5Biome10initBiomesEv'))
sp_main = e.alloc(16); sp_zoom = e.alloc(16)
e.call(e.sym('_ZN5Layer16getDefaultLayersExRSt10shared_ptrIS_ES2_13GeneratorType'), seed & 0xffffffff, 0, sp_main, sp_zoom, 1)
vt2name={}
for k,v in e.syms.items():
    if k.startswith('_ZTV'): vt2name[v+8]=subprocess.run(['c++filt',k],capture_output=True,text=True).stdout.strip().replace('vtable for ','')
def isLayer(p): return p and 0x30000000<=p<0x38000000 and e.r32(p) in vt2name and '_Sp_' not in vt2name[e.r32(p)]
bufs = e.alloc(16); b1 = e.alloc(0x10000); b2 = e.alloc(0x10000)
def ints(p):
    e.w32(bufs, b1); e.w32(bufs+4, b2)
    e.call(e.r32(e.r32(p)+0xc), p, bufs, X, Z, W, H)
    o=e.r32(bufs); v=[e.r32(o+4*i) for i in range(W*H)]
    return [a-(1<<32) if a>=1<<31 else a for a in v]
def walk(p, path):
    nm=vt2name[e.r32(p)].split('<')[0]
    print(path, nm, ' '.join(map(str,ints(p))))
    kids=[e.r32(p+o) for o in (0x24,0x2c,0x34) if isLayer(e.r32(p+o)) and e.r32(p+o)!=p]
    for i,k in enumerate(kids): walk(k, path+str(i))
walk(e.r32(sp_main), 'r')
