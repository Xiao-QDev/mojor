import emu, fakeblocks, subprocess, struct
e = emu.Emu()
ok, bad = e.run_init_array(verbose=True)
if bad: raise SystemExit(f'init_array 失败：{bad}/{ok + bad} 个构造函数未完成')
fakeblocks.install(e)
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
inv={v:k for k,v in e.syms.items()}
gb = e.sym('_ZN5Biome8getBiomeEiPS_')
ptr2id={}
for i in range(256):
    p=e.call(gb,i,0)
    if p: ptr2id.setdefault(p,i)
K={'Biome':'Default','ExtremeHillsBiome':'ExtremeHills','TaigaBiome':'Taiga','SwampBiome':'Swamp',
   'MutatedSavannaBiome':'MutatedSavanna','MesaBiome':'Mesa','MutatedBiome':'Mutated'}
for i in range(256):
    p=e.call(gb,i,0)
    if not p: continue
    vt=e.r32(p); bs=inv[e.r32(vt+0x3c)]
    cls=subprocess.run(['c++filt',bs],capture_output=True,text=True).stdout.split('::')[0]
    kind=K[cls]
    t=e.call_f(e.r32(vt+0x18), p)
    tb=struct.unpack('<I',struct.pack('<f',t))[0]
    top,td,fil=[e.uc.mem_read(p+o,1)[0] for o in (0x44,0x45,0x46)]
    extra=e.r32(p+0x9c)
    if kind=='Mutated': arg=ptr2id[extra]
    elif kind in ('ExtremeHills','Taiga'): arg=extra
    else: arg=0
    print(f'        {i} => S {{ kind: K::{kind}, arg: {arg}, top: {top}, top_data: {td}, filler: {fil}, temp: f32::from_bits(0x{tb:08x}) }},')
