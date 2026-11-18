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
# 返回值在 x87 ST0 的导入函数；其余（含 malloc 等）按整型/void 处理
FLOAT_RET = {'floor', 'ceil', 'sqrt', 'sin', 'cos', 'exp', 'log', 'fabs',
             'floorf', 'ceilf', 'sqrtf', 'sinf', 'cosf', 'fabsf', 'pow'}

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
        # 浮点返回值：fld qword [a+16]; ret（值由 hook 写入）
        # 整型/void：直接 ret，避免无谓的 fld 污染 x87 栈
        if name in FLOAT_RET:
            code = b'\xdd\x05' + struct.pack('<I', a + 16) + b'\xc3'
        else:
            code = b'\xc3'
        self.uc.mem_write(a, code)
        self.stubs[a] = name; self.stub_by_name[name] = a
        return a

    # ---- 内存 ----
    def alloc(self, n, zero=True):
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
        else:
            self.unknown.add(name)
        if fret is not None:
            uc.mem_write(addr + 16, struct.pack('<d', fret))
        uc.reg_write(UC_X86_REG_EAX, eax)

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
        eip = self.uc.reg_read(UC_X86_REG_EIP)
        if eip != RET_MAGIC:
            raise RuntimeError('call %#x 未返回到 %#x（停在 %#x，count=%d）'
                               % (addr, RET_MAGIC, eip, count))
        return self.uc.reg_read(UC_X86_REG_EAX)

    def run_init_array(self, verbose=False):
        sec = self.elf.get_section_by_name('.init_array')
        n = sec['sh_size'] // 4; base = sec['sh_addr']
        ok = bad = 0
        for i in range(n):
            f = self.r32(base + 4 * i)
            if f in (0, 0xffffffff):
                continue
            try:
                self.call(f, count=5_000_000); ok += 1
            except Exception as ex:
                bad += 1
                if verbose: print('init fail', hex(f), ex)
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
