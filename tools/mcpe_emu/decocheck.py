"""用原版 Biome::decorate 装饰 Rust 导出的 3×3 区块，与 Rust 的装饰结果逐方块比较。

用法: decocheck.py <seed> <cx> <cz> <dir>    （dir 由 `deco_dump` 生成：pre.bin / post.bin）

原版代码在 unicorn 里原样运行（Block::initBlocks、Biome::initBiomes 都是真的），
只有 BlockSource 的取/写方块等方法换成 Python 钩子，语义同 postProcess 里那个
BlockSource(..., false, true)：只能访问 3×3 区块、写方块不触发事件、高度图不更新。
"""
import os, sys, struct, subprocess, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import emu
from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_EAX

seed, CX, CZ, D = int(sys.argv[1]) & 0xffffffff, int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
CH = 32768 * 2 + 512


def load(path):
    raw = open(path, 'rb').read()
    w = {}
    i = 0
    for dz in (-1, 0, 1):
        for dx in (-1, 0, 1):
            c = raw[i:i + CH]; i += CH
            w[(CX + dx, CZ + dz)] = [bytearray(c[:32768]), bytearray(c[32768:65536]),
                                    bytearray(c[65536:65792]), bytearray(c[65792:66048])]
    return w


W = load(D + '/pre.bin')
RUST = load(D + '/post.bin')

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

MB = S('_ZN5Block7mBlocksE')
blockptr = [e.r32(MB + 4 * i) for i in range(256)]
matptr = [e.call(S('_ZNK5Block11getMaterialEv'), b) if b else 0 for b in blockptr]
lightblock = bytes(e.uc.mem_read(S('_ZN5Block11mLightBlockE'), 256))
matbm = {}
for i in range(256):
    m = matptr[i]
    matbm[i] = (e.r32(m), e.call(S('_ZNK8Material15getBlocksMotionEv'), m) & 0xff)
GB = S('_ZN5Biome8getBiomeEiPS_')
biomeptr = {}


def biome_of(bid):
    if bid not in biomeptr:
        biomeptr[bid] = e.call(GB, bid, 0)
    return biomeptr[bid]


SKY = S('_ZN10LightLayer3SKYE') if '_ZN10LightLayer3SKYE' in e.syms else None


# ---------------------------------------------------------------- 世界访问
def chunk(x, z):
    k = (x >> 4, z >> 4)
    if abs(k[0] - CX) > 1 or abs(k[1] - CZ) > 1:
        return None
    return W.get(k)


def idx(x, y, z):
    return ((x & 15) << 11) | ((z & 15) << 7) | y


def get(x, y, z):
    if not 0 <= y < 128:
        return 0
    c = chunk(x, z)
    return c[0][idx(x, y, z)] if c else 0


def data(x, y, z):
    if not 0 <= y < 128:
        return 0
    c = chunk(x, z)
    return c[1][idx(x, y, z)] if c else 0


def setb(x, y, z, i, d):
    if not 0 <= y <= 127:
        return 0
    c = chunk(x, z)
    if not c:
        return 0
    k = idx(x, y, z)
    oi, od = c[0][k], c[1][k]
    c[0][k] = i & 0xff
    c[1][k] = d & 15
    if oi == i:
        return 1 if od != (d & 15) else 0
    return 1


def heightmap(x, z):
    c = chunk(x, z)
    return c[2][(x & 15) | ((z & 15) << 4)] if c else 0


def top_solid(x, z):
    if not chunk(x, z):
        return 255
    h = 127
    while True:
        i = get(x, h, z); h -= 1
        mt, bm = matbm[i]
        if not ((mt == 7 or not bm) and h != 0):
            break
    return h + 2 if h else 0


def top_rain(x, z):
    y = 127
    while y > 0:
        i = get(x, y, z)
        mt, bm = matbm[i]
        if bm or mt in (5, 6):
            return min(y + 1, 127)
        y -= 1
    return -1


def sky(x, y, z):
    light = 15
    yy = 127
    while yy >= y:
        light -= lightblock[get(x, yy, z)]
        if light <= 0:
            return 0
        yy -= 1
    return light


# ---------------------------------------------------------------- 钩子
def pos(p):
    return tuple(struct.unpack('<3i', e.uc.mem_read(p, 12)))


def rb(p):
    """BlockID / FullBlock 按引用传（非平凡类型）"""
    return e.uc.mem_read(p, 1)[0]


def s32(v):
    return v - (1 << 32) if v >= 1 << 31 else v


QUEUE = e.alloc(0xa00)
FAKE = e.alloc(0x4000)   # 假 Level/Dimension：Level::isClientSide 读 +0x1540
calls = collections.Counter()
dbgc = collections.Counter()


def impl(name, a):
    """返回 eax（sret 的由钩子自己写内存）"""
    if name.startswith('BlockSource::getBlockID('):
        x, y, z = pos(a(2)) if 'BlockPos' in name else (s32(a(2)), s32(a(3)), s32(a(4)))
        e.uc.mem_write(a(0), bytes([get(x, y, z)])); return a(0)
    if name.startswith('BlockSource::getBlockAndData('):
        x, y, z = pos(a(2)) if 'BlockPos' in name else (s32(a(2)), s32(a(3)), s32(a(4)))
        e.uc.mem_write(a(0), bytes([get(x, y, z), data(x, y, z)])); return a(0)
    if name.startswith('BlockSource::getData('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return data(x, y, z)
    if name.startswith('BlockSource::getBlock('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return blockptr[get(x, y, z)]
    if name.startswith('BlockSource::getMaterial('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return matptr[get(x, y, z)]
    if name.startswith('BlockSource::isEmptyBlock('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return 1 if get(x, y, z) == 0 else 0
    if name == 'BlockSource::setBlockAndData(BlockPos const&, BlockID, unsigned char, int)':
        x, y, z = pos(a(1)); return setb(x, y, z, rb(a(2)), a(3) & 0xff)
    if name == 'BlockSource::setBlockAndData(BlockPos const&, FullBlock, int)':
        if os.environ.get('DBG') and calls[name.split('(')[0]] < 5: print('SBD', [hex(a(i)) for i in range(5)], pos(a(1)))
        x, y, z = pos(a(1)); return setb(x, y, z, rb(a(2)), rb(a(2) + 1))
    if name == 'BlockSource::setBlockAndData(int, int, int, BlockID, unsigned char, int)':
        return setb(s32(a(1)), s32(a(2)), s32(a(3)), rb(a(4)), a(5) & 0xff)
    if name == 'BlockSource::setBlockAndData(int, int, int, FullBlock, int)' or \
            name == 'BlockSource::setBlockAndDataNoUpdate(int, int, int, FullBlock)':
        return setb(s32(a(1)), s32(a(2)), s32(a(3)), rb(a(4)), rb(a(4) + 1))
    if name == 'BlockSource::setBlock(BlockPos const&, BlockID, int)':
        x, y, z = pos(a(1)); return setb(x, y, z, rb(a(2)), 0)
    if name in ('BlockSource::setBlock(int, int, int, BlockID, int)', 'BlockSource::setBlockNoUpdate(int, int, int, BlockID)'):
        return setb(s32(a(1)), s32(a(2)), s32(a(3)), rb(a(4)), 0)
    if name.startswith('BlockSource::removeBlock('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return setb(x, y, z, 0, 0)
    if name == 'BlockSource::getHeightmap(BlockPos const&)':
        x, y, z = pos(a(1)); return heightmap(x, z)
    if name == 'BlockSource::getHeightmap(int, int)':
        return heightmap(s32(a(1)), s32(a(2)))
    if name == 'BlockSource::getHeightmapPos(BlockPos const&)':
        x, y, z = pos(a(2)); e.uc.mem_write(a(0), struct.pack('<3i', x, heightmap(x, z), z)); return a(0)
    if name == 'BlockSource::getTopRainBlockPos(BlockPos const&)':
        x, y, z = pos(a(2)); e.uc.mem_write(a(0), struct.pack('<3i', x, top_rain(x, z), z)); return a(0)
    if name == 'BlockSource::getTopSolidBlock(BlockPos const&, bool)':
        x, y, z = pos(a(1)); return top_solid(x, z)
    if name == 'BlockSource::getTopSolidBlock(int, int, bool)':
        return top_solid(s32(a(1)), s32(a(2)))
    if name == 'BlockSource::getBiome(BlockPos const&)':
        x, y, z = pos(a(1)); c = chunk(x, z)
        return biome_of(c[3][(x & 15) | ((z & 15) << 4)] if c else 1)
    if name == 'BlockSource::getTickQueue(BlockPos const&)':
        return QUEUE
    if name.startswith('BlockSource::getRawBrightness('):
        x, y, z = pos(a(2)) if 'BlockPos' in name else (s32(a(2)), s32(a(3)), s32(a(4)))
        e.uc.mem_write(a(0), bytes([sky(x, y, z)])); return a(0)
    if name.startswith('BlockSource::getBrightness(LightLayer'):
        x, y, z = pos(a(3)) if 'BlockPos' in name else (s32(a(3)), s32(a(4)), s32(a(5)))
        v = sky(x, y, z) if a(2) == SKY else 0
        e.uc.mem_write(a(0), bytes([v])); return a(0)
    if name.startswith('BlockSource::canSeeSky('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return 1 if y >= heightmap(x, z) else 0
    if name == 'BlockSource::hasChunksAt(BlockPos const&, BlockPos const&)':
        (x0, _, z0), (x1, _, z1) = pos(a(1)), pos(a(2))
        return int(all(chunk(xx * 16, zz * 16) for xx in range(x0 >> 4, (x1 >> 4) + 1) for zz in range(z0 >> 4, (z1 >> 4) + 1)))
    if name == 'BlockSource::hasChunksAt(BlockPos const&, int)':
        x, y, z = pos(a(1)); r = s32(a(2))
        return int(all(chunk(xx * 16, zz * 16) for xx in range((x - r) >> 4, ((x + r) >> 4) + 1) for zz in range((z - r) >> 4, ((z + r) >> 4) + 1)))
    if name == 'BlockSource::hasChunksAt(int, int, int, int)':
        x, z, r = s32(a(1)), s32(a(3)), s32(a(4))
        return int(all(chunk(xx * 16, zz * 16) for xx in range((x - r) >> 4, ((x + r) >> 4) + 1) for zz in range((z - r) >> 4, ((z + r) >> 4) + 1)))
    if name.startswith('BlockSource::hasBlock('):
        x, y, z = pos(a(1)) if 'BlockPos' in name else (s32(a(1)), s32(a(2)), s32(a(3)))
        return 1 if chunk(x, z) else 0
    if name in ('BlockSource::getGrassColor(BlockPos const&)',):
        return 0
    if name in ('BlockSource::setGrassColor(int, BlockPos const&, int)',):
        return 0
    if name in ('BlockSource::getDimensionConst() const', 'BlockSource::getDimension()', 'BlockSource::getDimension() const',
                'BlockSource::getLevel()', 'BlockSource::getLevelConst() const', 'BlockSource::getLevel() const'):
        return FAKE
    return None


SRET = {'getBlockID', 'getBlockAndData', 'getHeightmapPos', 'getRawBrightness', 'getTopRainBlockPos', 'getBrightness(LightLayer', 'getTopBlock', '_getSkyDarken', 'clip'}
# 这些原版实现只调用上面钩住的方法，原样运行
NATIVE = ('isSolidBlockingBlock', 'isTopSolidBlocking', 'shouldFireEvents', 'getPublicSource', 'isOwnerThread', 'getDimensionId',
          'updateNeighborsAt', 'neighborChanged')
names = sorted(n for n in e.syms if n.startswith('_ZN11BlockSource') or n.startswith('_ZNK11BlockSource'))
dem = subprocess.run(['c++filt'], input='\n'.join(names), capture_output=True, text=True).stdout.split('\n')
by_addr = {}
for n, d in zip(names, dem):
    if any(k in d for k in NATIVE) or 'BlockSource::BlockSource' in d or '~' in d:
        continue
    by_addr[e.syms[n]] = d


def hook(uc, addr, size, _):
    name = by_addr[addr]
    esp = uc.reg_read(UC_X86_REG_ESP)
    a = lambda i: e.r32(esp + 4 + 4 * i)
    if os.environ.get('DBG') and name.startswith('BlockSource::set'):
        dbgc[name] += 1
        if dbgc[name] <= 3:
            print('DBG', name, [hex(a(i)) for i in range(6)])
    v = impl(name, a)
    if v is None:
        raise RuntimeError('未实现的 BlockSource 方法: ' + name)
    calls[name.split('(')[0]] += 1
    sret = any(('::' + k) in name for k in SRET)
    ret = e.r32(esp)
    uc.reg_write(UC_X86_REG_EAX, v & 0xffffffff)
    uc.reg_write(UC_X86_REG_ESP, esp + (8 if sret else 4))
    uc.reg_write(UC_X86_REG_EIP, ret)


for addr in by_addr:
    e.uc.hook_add(UC_HOOK_CODE, hook, begin=addr, end=addr)


# ---------------------------------------------------------------- Random（Python 里也能推进，用于 buildGrassColor）
def mt_seed(s):
    mt = [s & 0xffffffff]
    for i in range(1, 624):
        mt.append((0x6c078965 * (mt[-1] ^ (mt[-1] >> 30)) + i) & 0xffffffff)
    return mt


def rnd_new(s):
    r = e.alloc(0xa00)
    e.w32(r, s)
    for i, v in enumerate(mt_seed(s)):
        e.w32(r + 4 + 4 * i, v)
    e.w32(r + 0x9c4, 624)
    return r


def rnd_genrand(r):
    mti = e.r32(r + 0x9c4)
    mt = list(struct.unpack('<624I', e.uc.mem_read(r + 4, 2496)))
    if mti >= 624:
        for k in range(624):
            y = (mt[k] & 0x80000000) | (mt[(k + 1) % 624] & 0x7fffffff)
            mt[k] = mt[(k + 397) % 624] ^ (y >> 1) ^ (0x9908b0df if y & 1 else 0)
        mti = 0
        e.uc.mem_write(r + 4, struct.pack('<624I', *mt))
    y = mt[mti]; mti += 1
    e.w32(r + 0x9c4, mti)
    y ^= y >> 11; y ^= (y << 7) & 0x9d2c5680; y ^= (y << 15) & 0xefc60000; y ^= y >> 18
    return y


# 先确认 Python 版 MT 与原版 Random::_genRandInt32 一致
_t = rnd_new(12345)
_r = [e.call(S('_ZN6Random13genrand_int32Ev'), _t) for _ in range(1000)] if '_ZN6Random13genrand_int32Ev' in e.syms else None
_t2 = rnd_new(12345)
if _r is not None:
    assert _r == [rnd_genrand(_t2) for _ in range(1000)], 'MT 不一致'

BUILD_GRASS = S('_ZN12FoliageColor15buildGrassColorERK8BlockPosS2_R11BlockSourceR6Random')
RNG = None


# buildGrassColor 只改草色，但每列消耗一次 random.nextInt(8)：
# 换成一段机器码，循环调用原版 genrand_int32 256 次（Python 版 MT 与原版对象的下标语义不同，不能代推）
_gen = S('_ZN6Random13genrand_int32Ev')
_code = bytearray(b'\x56\x8b\x74\x24\x14\xb9\x00\x01\x00\x00')
_loop = len(_code)
_code += b'\x51\x56\xe8'
_call_end = BUILD_GRASS + len(_code) + 4
_code += struct.pack('<i', _gen - _call_end)
_code += b'\x83\xc4\x04\x59'
_code += b'\xe2' + struct.pack('<b', _loop - (len(_code) + 2))
_code += b'\x5e\x31\xc0\xc3'
e.uc.mem_write(BUILD_GRASS, bytes(_code))

# TRACE=1：打印各 Feature::place 的调用（类名、坐标）
if os.environ.get('TRACE'):
    _places = {}
    for n, a in e.syms.items():
        if n.endswith('5placeER11BlockSourceRK8BlockPosR6Random') and 'Feature' in n:
            _places[a] = subprocess.run(['c++filt', n], capture_output=True, text=True).stdout.strip().split('::')[0]

    def trace_hook(uc, addr, size, _):
        esp = uc.reg_read(UC_X86_REG_ESP)
        p_ = pos(e.r32(esp + 12))
        rr = e.r32(esp + 16)
        print('TRACE', _places[addr], p_, e.r32(rr + 0x9c4))
        if os.environ.get('TRACE2') and 'GroundBush' in _places[addr]:
            print('BUSH', p_, [get(p_[0], y, p_[2]) for y in range(55, 70)])

    for a in _places:
        e.uc.hook_add(UC_HOOK_CODE, trace_hook, begin=a, end=a)

# GENLOG=1：记录每次 genrand_int32 的调用点（函数+偏移）
if os.environ.get('GENLOG'):
    import bisect
    _starts = sorted((a, n) for n, a in e.syms.items())

    def _gen_hook(uc, addr, size, _):
        ret = e.r32(uc.reg_read(UC_X86_REG_ESP))
        k = bisect.bisect_right(_starts, (ret, '\xff')) - 1
        print('GEN', e.r32(e.r32(uc.reg_read(UC_X86_REG_ESP) + 4) + 0x9c4), _starts[k][1][:60] + '+' + hex(ret - _starts[k][0]))

    e.uc.hook_add(UC_HOOK_CODE, _gen_hook, begin=_gen, end=_gen)
    e.uc.ctl_remove_cache(_gen, _gen + 0x40)   # 之前已翻译过的块不会带上新钩子

# ---------------------------------------------------------------- 跑 decorate
i1 = (((rnd_genrand(_t3 := rnd_new(seed)) >> 1) // 2) * 2 + 1) & 0xffffffff
j1 = (((rnd_genrand(_t3) >> 1) // 2) * 2 + 1) & 0xffffffff
mix = ((CZ * j1) + (CX * i1)) & 0xffffffff
RNG = rnd_new(seed ^ mix)
if os.environ.get('TRACE2'):
    _sv = bytes(e.uc.mem_read(RNG, 0xa00)); print('START', hex(seed ^ mix), [rnd_genrand(RNG) for _ in range(3)]); e.uc.mem_write(RNG, _sv)
    _c = e.alloc(0xa00); e.uc.mem_write(_c, _sv); print('NATIVE', [e.call(S('_ZN6Random13genrand_int32Ev'), _c) for _ in range(3)]); _d = e.alloc(0xa00); e.uc.mem_write(_d, bytes(e.uc.mem_read(_c, 0xa00))); print('PEEK', rnd_genrand(_d), e.call(S('_ZN6Random13genrand_int32Ev'), _c), e.r32(_c + 0x9c4), [hex(e.r32(_c + o)) for o in (0, 4, 0x9c0, 0x9c4, 0x9c8, 0x9cc)])
center = W[(CX, CZ)]
bid = center[3][15 | (15 << 4)]
b = biome_of(bid)
vt = e.r32(b)
dec = S('_ZN5Biome8decorateEP11BlockSourceR6RandomRK8BlockPosbf')
# decorate 在 vtable 里的槽位：用基类 Biome（海洋）的 vtable 找
ovt = e.r32(biome_of(0))
slot = next(i for i in range(200) if e.r32(ovt + 4 * i) == dec)
src = e.alloc(0x200)
p = e.alloc(12)
e.uc.mem_write(p, struct.pack('<3i', CX * 16, 0, CZ * 16))
FLT_MAX = 0x7f7fffff
_df = e.r32(vt + 4 * slot)
if os.environ.get('TRACE'):
    import bisect
    _st = sorted((a, n) for n, a in e.syms.items())
    print('DECORATE', subprocess.run(['c++filt', _st[bisect.bisect_right(_st, (_df, '\xff')) - 1][1]], capture_output=True, text=True).stdout.strip())
e.call(_df, b, src, RNG, p, 0, FLT_MAX, count=2_000_000_000)

if os.environ.get('SOLID'):
    _sol = S('_ZN11LiquidBlock8solidifyER11BlockSourceRK8BlockPosS4_')

    def _sol_hook(uc, addr, size, _):
        esp = uc.reg_read(UC_X86_REG_ESP)
        p1, p2 = pos(e.r32(esp + 12)), pos(e.r32(esp + 16))
        if abs(p1[0] - int(os.environ['SOLID'])) < 300:
            print('SOLIDIFY', blockptr.index(e.r32(esp + 4)) if e.r32(esp + 4) in blockptr else hex(e.r32(esp + 4)), p1, p2, get(*p1), get(*p2))

    e.uc.hook_add(UC_HOOK_CODE, _sol_hook, begin=_sol, end=_sol)
    _cnt = collections.Counter()
    for _n in ('_ZN11BlockSource15neighborChangedERK8BlockPosS2_7BlockID', '_ZN17LiquidBlockStatic15neighborChangedER11BlockSourceRK8BlockPosS4_',
               '_ZN11BlockSource17updateNeighborsAtERK8BlockPos7BlockID', '_ZNK5Level12isClientSideEv'):
        _a = S(_n)
        e.uc.hook_add(UC_HOOK_CODE, (lambda n: (lambda uc, addr, size, _: _cnt.update([n])))(_n), begin=_a, end=_a)
        e.uc.ctl_remove_cache(_a, _a + 0x40)
    import atexit
    atexit.register(lambda: print('SOLIDCNT', _cnt))
    e.uc.ctl_remove_cache(_sol, _sol + 0x40)

# LIQUID=1：再跑 _fixWaterAlongEdges（Python 照抄）+ 原版 BlockTickingQueue::tickAllPendingTicks
if os.environ.get('LIQUID'):
    QADD = S('_ZN17BlockTickingQueue3addER11BlockSourceRK8BlockPos7BlockIDi')
    qp = e.alloc(12); qid = e.alloc(4)
    edges = [(0, z) for z in range(16)] + [(x, zz) for x in range(1, 15) for zz in (0, 15)] + [(15, z) for z in range(16)]
    edges.sort(key=lambda t: (t[0], t[1]) if t[0] in (0, 15) else (t[0], t[1]))
    order = [(0, z) for z in range(16)]
    for x in range(1, 15):
        order += [(x, 0), (x, 15)]
    order += [(15, z) for z in range(16)]
    for dx, dz in order:
        x, z = CX * 16 + dx, CZ * 16 + dz
        prev = True
        for y in range(heightmap(x, z)):
            if get(x, y, z) == 9:
                if prev:
                    prev = False
                    setb(x, y, z, 8, 0)
                    e.uc.mem_write(qp, struct.pack('<3i', x, y, z)); e.uc.mem_write(qid, bytes([8]))
                    e.call(QADD, QUEUE, src, qp, qid, 1)
            else:
                prev = True
    e.call(S('_ZN17BlockTickingQueue19tickAllPendingTicksER11BlockSource'), QUEUE, src, count=2_000_000_000)

# ---------------------------------------------------------------- 比较
diff = collections.Counter()
cols = collections.defaultdict(list)
total = 0
for k in W:
    for i in range(32768):
        gi, ri = W[k][0][i], RUST[k][0][i]
        gd, rd = W[k][1][i], RUST[k][1][i]
        if gi != ri or gd != rd:
            total += 1
            diff[(ri, gi)] += 1
            x = k[0] * 16 + (i >> 11); z = k[1] * 16 + ((i >> 7) & 15); y = i & 127
            cols[(x, z)].append((y, (ri, rd), (gi, gd)))
print(calls)
print(f'seed {seed} chunk {CX},{CZ} biome {bid}: 方块差异 {total}   (rust,orig): {diff.most_common(10)}')
for (x, z), v in sorted(cols.items())[:int(os.environ.get('SHOW', '12'))]:
    print(' ', x, z, sorted(v)[:6])
open(D + '/orig_post.bin', 'wb').write(b''.join(bytes(W[(CX + dx, CZ + dz)][0]) + bytes(W[(CX + dx, CZ + dz)][1])
                                                for dz in (-1, 0, 1) for dx in (-1, 0, 1)))
