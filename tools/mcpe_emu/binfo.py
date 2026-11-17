import emu, subprocess, struct
e = emu.Emu(); e.run_init_array()
import fakeblocks; fakeblocks.install(e)
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
inv={v:k for k,v in e.syms.items()}
dm=lambda a: subprocess.run(['c++filt',inv.get(a,hex(a))],capture_output=True,text=True).stdout.strip().split('(')[0]
gb = e.sym('_ZN5Biome8getBiomeEiPS_')
for i in range(256):
    p = e.call(gb, i, 0)
    if not p: continue
    vt=e.r32(p)
    bs=e.r32(vt+0x3c); tf=e.r32(vt+0x18)
    t=e.call_f(tf, p)
    top,td,fil=[e.uc.mem_read(p+o,1)[0] for o in (0x44,0x45,0x46)]
    print(i, dm(bs), 'top',top,td,'fill',fil,'temp',repr(t), dm(tf))
