"""在 unicorn 里加载 x86 libminecraftpe.so，直接调用其中的函数取真值。"""
import struct, math, sys, os
from elftools.elf.elffile import ELFFile
from elftools.elf.relocation import RelocationSection
from unicorn import *
from unicorn.x86_const import *

SO = sys.argv[1] if len(sys.argv) > 1 and sys.argv[1].endswith('.so') else \
    os.environ.get('MCPE_SO', 'libminecraftpe.so')
STUB = 0x20000000
HEAP = 0x30000000
STACK = 0x40000000
RET_MAGIC = 0x50000000

class Emu:
    def __init__(self, path=SO):
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        f = open(path, 'rb'); self.elf = elf = ELFFile(f)
        hi = 0
        for s in elf.iter_segments():
            if s['p_type'] == 'PT_LOAD':
                hi = max(hi, s['p_vaddr'] + s['p_memsz'])
        uc.mem_map(0, (hi + 0xfff) & ~0xfff)
        for s in elf.iter_segments():
            if s['p_type'] == 'PT_LOAD':
                uc.mem_write(s['p_vaddr'], s.data())
        uc.mem_map(STUB, 0x100000); uc.mem_map(HEAP, 0x8000000)
        uc.mem_map(STACK - 0x200000, 0x200000); uc.mem_map(RET_MAGIC, 0x1000)
        uc.mem_write(RET_MAGIC, b'\xf4')  # hlt
        self.heap = HEAP
        self.heap_limit = 0
        dsym = elf.get_section_by_name('.dynsym')
        self.syms = {}
        for s in dsym.iter_symbols():
            if s['st_value'] and s['st_shndx'] != 'SHN_UNDEF':
                self.syms.setdefault(s.name, s['st_value'])
        self.stubs = {}   # addr -> name
        self.stub_by_name = {}
        self.nstub = 0
        for sec in elf.iter_sections():
            if not isinstance(sec, RelocationSection):
                continue
            symtab = elf.get_section(sec['sh_link'])
            for r in sec.iter_relocations():
                t = r['r_info_type']; off = r['r_offset']
                if t == 8:   # R_386_RELATIVE：基址 0，不用改
                    continue
                sym = symtab.get_symbol(r['r_info_sym'])
                if sym['st_shndx'] != 'SHN_UNDEF' and sym['st_value']:
                    val = sym['st_value']
                else:
                    val = self.stub_for(sym.name)
                if t in (6, 7):
                    uc.mem_write(off, struct.pack('<I', val))
                elif t == 1:
                    a = struct.unpack('<I', uc.mem_read(off, 4))[0]
                    uc.mem_write(off, struct.pack('<I', (a + val) & 0xffffffff))
                elif t == 2:  # PC32
                    a = struct.unpack('<I', uc.mem_read(off, 4))[0]
                    uc.mem_write(off, struct.pack('<I', (a + val - off) & 0xffffffff))
        uc.hook_add(UC_HOOK_CODE, self._stub_hook, begin=STUB, end=STUB + 0x100000)
        self.unknown = set()
        self.patches = {}

    def stub_for(self, name):
        if name in self.stub_by_name:
            return self.stub_by_name[name]
        a = STUB + self.nstub * 32; self.nstub += 1
        # 每个桩：fld qword [a+16]; ret   （浮点返回值放 ST0；其余函数忽略 ST0）
        code = b'\xdd\x05' + struct.pack('<I', a + 16) + b'\xc3'
        if name == '_ZNKSt8__detail20_Prime_rehash_policy14_M_need_rehashEjjj':
            code = b'\xdd\x05' + struct.pack('<I', a + 16) + b'\xc2\x04\x00'   # sret：ret 4
        if 'S_empty_rep_storage' in name:
            code = b'\0' * 32   # 数据符号：空字符串的 _Rep（长度 0）
        elif name in ('_tolower_tab_', '_toupper_tab_'):
            # bionic：const short* 指向 257 项表（下标 c+1）
            conv = (lambda c: ord(chr(c).lower()) if 65 <= c <= 90 else c) if 'lower' in name else \
                   (lambda c: ord(chr(c).upper()) if 97 <= c <= 122 else c)
            tab = self.alloc(2 * 257)
            self.uc.mem_write(tab, struct.pack('<h', -1) + b''.join(struct.pack('<h', conv(c)) for c in range(256)))
            code = struct.pack('<I', tab) + b'\0' * 28
        self.uc.mem_write(a, code)
        self.stubs[a] = name; self.stub_by_name[name] = a
        return a

    # ---- 内存 ----
    def alloc(self, n, zero=True):
        if n > 0x1000000 or (self.heap_limit and self.heap + n > self.heap_limit):
            raise MemoryError('alloc 0x%x' % n)   # 模拟器里算错的尺寸：让该调用失败
        a = (self.heap + 15) & ~15; self.heap = a + max(n, 1)
        if zero: self.uc.mem_write(a, b'\0' * max(n, 1))
        return a
    def r32(self, a): return struct.unpack('<I', self.uc.mem_read(a, 4))[0]
    def w32(self, a, v): self.uc.mem_write(a, struct.pack('<I', v & 0xffffffff))
    def rf(self, a, n=1): return list(struct.unpack('<%df' % n, self.uc.mem_read(a, 4 * n)))
    def wf(self, a, vals): self.uc.mem_write(a, struct.pack('<%df' % len(vals), *vals))

    def arg(self, i):
        esp = self.uc.reg_read(UC_X86_REG_ESP)
        return self.r32(esp + 4 + 4 * i)
    def argd(self, i):
        esp = self.uc.reg_read(UC_X86_REG_ESP)
        return struct.unpack('<d', self.uc.mem_read(esp + 4 + 4 * i, 8))[0]
    def argf(self, i):
        esp = self.uc.reg_read(UC_X86_REG_ESP)
        return struct.unpack('<f', self.uc.mem_read(esp + 4 + 4 * i, 4))[0]

    def _stub_hook(self, uc, addr, size, _):
        name = self.stubs.get(addr)
        if name is None:
            return
        eax = 0; fret = None
        if name in ('_Znwj', '_Znaj', 'malloc'):
            eax = self.alloc(self.arg(0))
        elif name == 'calloc':
            eax = self.alloc(self.arg(0) * self.arg(1))
        elif name in ('memset',):
            d, c, n = self.arg(0), self.arg(1), self.arg(2)
            uc.mem_write(d, bytes([c & 0xff]) * n); eax = d
        elif name in ('memcpy', 'memmove'):
            d, s, n = self.arg(0), self.arg(1), self.arg(2)
            uc.mem_write(d, bytes(uc.mem_read(s, n))); eax = d
        elif name in ('floor', 'ceil', 'sqrt', 'sin', 'cos', 'exp', 'log', 'fabs'):
            fret = getattr(math, name)(self.argd(0))
        elif name in ('floorf', 'ceilf', 'sqrtf', 'sinf', 'cosf', 'fabsf'):
            fret = getattr(math, name[:-1])(self.argf(0))
        elif name == 'pow':
            fret = math.pow(self.argd(0), self.argd(2))
        elif name == '_ZNKSt8__detail20_Prime_rehash_policy11_M_next_bktEj':
            n = max(self.arg(1), 2)
            if n > 100000:      # 模拟器里浮点负载因子算出的离谱值：桶数只要 >0 且一致即可
                n = 1031
            while any(n % p == 0 for p in range(2, int(n ** 0.5) + 1)):
                n += 1
            eax = n
        elif name == '_ZNKSt8__detail20_Prime_rehash_policy14_M_need_rehashEjjj':
            self.w32(self.arg(0), 0); self.w32(self.arg(0) + 4, 0)   # 永不 rehash
            eax = self.arg(0)
        elif name == '_ZSt11_Hash_bytesPKvjj':
            h = self.arg(2)
            for c in bytes(uc.mem_read(self.arg(0), self.arg(1))):
                h = (h * 31 + c) & 0xffffffff
            eax = h
        elif name.startswith('_ZNSs') and self._string_op(name):
            eax = self._str_ret
        else:
            self.unknown.add(name)
        if fret is not None:
            uc.mem_write(addr + 16, struct.pack('<d', fret))
        uc.reg_write(UC_X86_REG_EAX, eax)

    # ---- libstdc++ COW std::string 的最小实现（对象 = 指向字符数据的指针，前 12 字节是 _Rep） ----
    def str_new(self, data):
        rep = self.alloc(12 + len(data) + 1)
        self.w32(rep, len(data)); self.w32(rep + 4, len(data)); self.w32(rep + 8, 0)
        self.uc.mem_write(rep + 12, bytes(data) + b'\0')
        return rep + 12
    def str_get(self, obj):
        p = self.r32(obj)
        if p == 0:
            return b''
        n = self.r32(p - 12)
        return bytes(self.uc.mem_read(p, n)) if n else b''
    def cstr(self, p, n=None):
        if n is not None:
            return bytes(self.uc.mem_read(p, n)) if n else b''
        out = bytearray()
        while True:
            c = self.uc.mem_read(p, 64)
            i = c.find(0)
            if i >= 0:
                out += c[:i]; return bytes(out)
            out += c; p += 64
    def _string_op(self, name):
        a = self.arg
        self._str_ret = a(0)
        if name in ('_ZNSsC1EPKcRKSaIcE', '_ZNSsC2EPKcRKSaIcE'):
            self.w32(a(0), self.str_new(self.cstr(a(1))))
        elif name in ('_ZNSsC1EPKcjRKSaIcE', '_ZNSsC2EPKcjRKSaIcE'):
            self.w32(a(0), self.str_new(self.cstr(a(1), a(2))))
        elif name in ('_ZNSsC1ERKSs', '_ZNSsC2ERKSs'):
            self.w32(a(0), self.str_new(self.str_get(a(1))))
        elif name in ('_ZNSsC1EjcRKSaIcE',):
            self.w32(a(0), self.str_new(bytes([a(2) & 0xff]) * a(1)))
        elif name == '_ZNSs6appendEPKcj':
            self.w32(a(0), self.str_new(self.str_get(a(0)) + self.cstr(a(1), a(2))))
        elif name == '_ZNSs6appendEPKc':
            self.w32(a(0), self.str_new(self.str_get(a(0)) + self.cstr(a(1))))
        elif name == '_ZNSs6appendERKSs':
            self.w32(a(0), self.str_new(self.str_get(a(0)) + self.str_get(a(1))))
        elif name == '_ZNSs6appendEjc':
            self.w32(a(0), self.str_new(self.str_get(a(0)) + bytes([a(2) & 0xff]) * a(1)))
        elif name == '_ZNSs9push_backEc':
            self.w32(a(0), self.str_new(self.str_get(a(0)) + bytes([a(1) & 0xff])))
        elif name == '_ZNSs6assignERKSs':
            self.w32(a(0), self.str_new(self.str_get(a(1))))
        elif name == '_ZNSs6assignEPKc':
            self.w32(a(0), self.str_new(self.cstr(a(1))))
        elif name == '_ZNSs6assignEPKcj':
            self.w32(a(0), self.str_new(self.cstr(a(1), a(2))))
        elif name == '_ZNSs6insertEjPKcj':
            s = self.str_get(a(0)); p = a(1)
            self.w32(a(0), self.str_new(s[:p] + self.cstr(a(2), a(3)) + s[p:]))
        elif name == '_ZNSs14_M_replace_auxEjjjc':
            s = self.str_get(a(0)); p, n1, n2, c = a(1), a(2), a(3), a(4) & 0xff
            self.w32(a(0), self.str_new(s[:p] + bytes([c]) * n2 + s[p + n1:]))
        elif name == '_ZNSs7replaceEjjPKcj':
            s = self.str_get(a(0)); p, n1 = a(1), a(2)
            self.w32(a(0), self.str_new(s[:p] + self.cstr(a(3), a(4)) + s[p + n1:]))
        elif name == '_ZNSs5eraseEjj':
            s = self.str_get(a(0)); p, n = a(1), a(2)
            self.w32(a(0), self.str_new(s[:p] + (s[p + n:] if n != 0xffffffff else b'')))
        elif name == '_ZNKSs7compareEPKc':
            x, y = self.str_get(a(0)), self.cstr(a(1)); self._str_ret = (x > y) - (x < y)
        elif name == '_ZNKSs7compareERKSs':
            x, y = self.str_get(a(0)), self.str_get(a(1)); self._str_ret = (x > y) - (x < y)
        elif name in ('_ZNSs7reserveEj', '_ZNSs12_M_leak_hardEv', '_ZNSs4_Rep10_M_disposeERKSaIcE',
                      '_ZNSs4_Rep10_M_destroyERKSaIcE', '_ZNSsD1Ev', '_ZNSsD2Ev'):
            pass
        elif name == '_ZNSs4swapERSs':
            x, y = self.r32(a(0)), self.r32(a(1)); self.w32(a(0), y); self.w32(a(1), x)
        else:
            return False
        return True

    def fix_hashmap(self, addr, nbkt=11):
        """静态构造失败的 std::unordered_map：补成空表（gnustl 布局）"""
        if self.r32(addr + 4) == 0:
            self.w32(addr, self.alloc(4 * nbkt)); self.w32(addr + 4, nbkt)
            self.w32(addr + 8, 0); self.w32(addr + 12, 0)
            self.wf(addr + 16, [1.0]); self.w32(addr + 20, 0)

    def patch_ret(self, addr, nargs_bytes=0):
        """把某函数改成直接返回（cdecl 调用者清栈）"""
        self.uc.mem_write(addr, b'\xc3')

    def patch_ret_this(self, addr):
        """改成返回第一个参数（链式调用的 setter）"""
        self.uc.mem_write(addr, b'\x8b\x44\x24\x04\xc3')

    def call(self, addr, *args, count=0):
        esp = STACK - 0x1000
        words = [RET_MAGIC] + [a & 0xffffffff for a in args]
        for i, w in enumerate(words):
            self.w32(esp + 4 * i, w)
        self.uc.reg_write(UC_X86_REG_ESP, esp)
        self.uc.emu_start(addr, RET_MAGIC, count=count)
        return self.uc.reg_read(UC_X86_REG_EAX)

    def run_init_array(self, verbose=False):
        sec = self.elf.get_section_by_name('.init_array')
        n = sec['sh_size'] // 4; base = sec['sh_addr']
        ok = bad = 0
        for i in range(n):
            f = self.r32(base + 4 * i)
            if f in (0, 0xffffffff):
                continue
            h0 = self.heap
            self.heap_limit = h0 + 0x1000000
            try:
                self.call(f, count=5_000_000); ok += 1
            except Exception as ex:
                bad += 1
                self.heap = h0
                if verbose: print('init fail', hex(f), ex)
        self.heap_limit = 0
        return ok, bad

    def call_f(self, addr, *args):
        """调用返回 float（x87 ST0）的函数：返回地址处放 fstp [slot]; hlt"""
        stub = RET_MAGIC + 0x100
        slot = RET_MAGIC + 0x200
        self.uc.mem_write(stub, b'\xd9\x1d' + struct.pack('<I', slot) + b'\xf4')
        esp = STACK - 0x1000
        words = [stub] + [a & 0xffffffff for a in args]
        for i, w in enumerate(words):
            self.w32(esp + 4 * i, w)
        self.uc.reg_write(UC_X86_REG_ESP, esp)
        self.uc.emu_start(addr, stub + 6)
        return struct.unpack('<f', self.uc.mem_read(slot, 4))[0]

    def sym(self, name):
        return self.syms[name]
