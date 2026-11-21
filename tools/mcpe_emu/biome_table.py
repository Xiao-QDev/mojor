import emu
e = emu.Emu(); e.run_init_array()
e.patch_ret_this(e.sym('_ZN5Biome7setNameERKSs'))
e.call(e.sym('_ZN5Biome10initBiomesEv'))
gb = e.sym('_ZN5Biome8getBiomeEiPS_')
dflt = e.call(gb, 0, 0)
lines=[]
for i in range(256):
    p = e.call(gb, i, 0)
    if p:
        d, s = e.r32(p+0x64), e.r32(p+0x68)
        lines.append(f'        {i} => (f32::from_bits(0x{d:08x}), f32::from_bits(0x{s:08x})),')
# 未注册 id：Biome::getBiome(id, default) 返回 default；fillRawBiomeData 传入的 default 见反汇编
print('\n'.join(lines))
