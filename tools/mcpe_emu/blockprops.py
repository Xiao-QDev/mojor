"""导出原版方块属性表（Block::initBlocks 之后的 mLightBlock / mSolid / 材质 / properties）。

输出 Rust 常量表，写到 src/level/generator/vanilla/blockprops_table.rs。
"""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
import emu

e = emu.Emu()
e.run_init_array()
e.call(e.sym('_ZN8Material13initMaterialsEv'))
e.fix_hashmap(e.sym('_ZN4Item14mItemLookupMapE'))
# 纹理图集在模拟器里不存在：取 UV 的函数改成直接返回（sret，ret 4）
for n, a in e.syms.items():
    if 'getTextureUVCoordinateSet' in n and n.startswith('_ZN5Block'):
        e.uc.mem_write(a, b'\x8b\x44\x24\x04\xc2\x04\x00')
import struct
_dummy = e.alloc(0x400)
e.uc.mem_write(e.sym('_ZNK12TextureAtlas14getTextureItemERKSs'), b'\xb8' + struct.pack('<I', _dummy) + b'\xc3')
e.call(e.sym('_ZN5Block10initBlocksEv'), count=200_000_000)
S = e.sym
mblocks = S('_ZN5Block7mBlocksE'); light = S('_ZN5Block11mLightBlockE'); solid = S('_ZN5Block6mSolidE')
rows = []
for i in range(256):
    b = e.r32(mblocks + 4 * i)
    if not b:
        rows.append(None); continue
    m = e.call(S('_ZNK5Block11getMaterialEv'), b)
    mt = e.r32(m)
    def mb(f): return e.call(S(f), m) & 0xff
    flags = (mb('_ZNK8Material15getBlocksMotionEv') | mb('_ZNK8Material7isSolidEv') << 1 |
             mb('_ZNK8Material8isLiquidEv') << 2 | mb('_ZNK8Material13isReplaceableEv') << 3 |
             mb('_ZNK8Material11isFlammableEv') << 4 | mb('_ZNK8Material15isSolidBlockingEv') << 5)
    props = e.call(S('_ZNK5Block13getPropertiesEv'), b)
    bsolid = e.call(S('_ZNK5Block7isSolidEv'), b) & 0xff
    lb = e.uc.mem_read(light + i, 1)[0]
    ms = e.uc.mem_read(solid + i, 1)[0]
    shape = e.call(S('_ZNK5Block13getBlockShapeEv'), b)
    shape = shape - (1 << 32) if shape >= 1 << 31 else shape
    wblk = e.call(S('_ZNK5Block15isWaterBlockingEv'), b) & 0xff
    rows.append((mt, flags, lb, ms, bsolid, props, shape, wblk))

out = sys.argv[1] if len(sys.argv) > 1 else 'blockprops_table.rs'
with open(out, 'w') as f:
    f.write('// 由 tools/mcpe_emu/blockprops.py 从原版 libminecraftpe.so 导出，勿手改。\n')
    f.write('// (材质类型, 材质标志[motion|solid<<1|liquid<<2|replaceable<<3|flammable<<4|solidBlocking<<5], '
            'mLightBlock, mSolid, Block::isSolid, properties, blockShape, isWaterBlocking)\n')
    f.write('pub const BLOCK_PROPS: [(u8, u8, u8, bool, bool, u32, i32, bool); 256] = [\n')
    tf = lambda v: 'true' if v else 'false'
    for i, r in enumerate(rows):
        mt, fl, lb, ms, bs, pr, sh, wb = r
        f.write('    (%d, %d, %d, %s, %s, 0x%x, %d, %s), // %d\n' % (mt, fl, lb, tf(ms), tf(bs), pr, sh, tf(wb), i))
    f.write('];\n')
    vines = [e.r32(S('_ZN9VineBlock%s' % n)) for n in ('10VINE_SOUTHE', '10VINE_NORTHE', '9VINE_EASTE', '9VINE_WESTE')]
    f.write('/// VineBlock::VINE_SOUTH / NORTH / EAST / WEST\n')
    f.write('pub const VINE_SOUTH: u8 = %d;\npub const VINE_NORTH: u8 = %d;\npub const VINE_EAST: u8 = %d;\n'
            'pub const VINE_WEST: u8 = %d;\n' % tuple(vines))
print('wrote', out)
