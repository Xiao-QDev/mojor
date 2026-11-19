"""导出各 biome 的 BiomeDecorator 计数器（原版 initBiomes 之后，decorator 在 Biome+4）。"""
import os, sys, struct
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import emu

e = emu.Emu()
e.run_init_array()
S = e.sym
e.call(S('_ZN8Material13initMaterialsEv'))
for n, a in e.syms.items():
    if 'getTextureUVCoordinateSet' in n and n.startswith('_ZN5Block'):
        e.uc.mem_write(a, b'\x8b\x44\x24\x04\xc2\x04\x00')
_dummy = e.alloc(0x400)
e.uc.mem_write(S('_ZNK12TextureAtlas14getTextureItemERKSs'), b'\xb8' + struct.pack('<I', _dummy) + b'\xc3')
e.fix_hashmap(S('_ZN4Item14mItemLookupMapE'))
e.call(S('_ZN5Block10initBlocksEv'), count=200_000_000)
e.call(S('_ZN5Biome10initBiomesEv'), count=200_000_000)
GB = S('_ZN5Biome8getBiomeEiPS_')
names = ['waterlily', 'tree', 'flowers', 'grass', 'deadbush', 'mushrooms', 'reeds', 'cactus', 'gravel', 'sand', 'clay', 'hugeMushroom']
print('id  ' + ' '.join('%9s' % n[:9] for n in names))
for bid in list(range(40)) + list(range(128, 168)):
    b = e.call(GB, bid, 0)
    if not b or (bid >= 128 and b == e.call(GB, bid - 128, 0)):
        continue
    d = e.r32(b + 4)
    v = list(struct.unpack('<ifiiiiiiiiii', e.uc.mem_read(d + 0xc8, 48)))
    print('%3d ' % bid + ' '.join('%9s' % (('%.3g' % x) if isinstance(x, float) else x) for x in v))
