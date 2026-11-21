"""反汇编原版函数并标注调用目标。用法: disasm.py <符号名或其 demangle 子串>"""
import sys, bisect, subprocess, os; sys.path.insert(0,'/home/user/mojor/tools/mcpe_emu')
import emu
from capstone import *
e=emu.Emu(); md=Cs(CS_ARCH_X86, CS_MODE_32)
st=sorted((a,n) for n,a in e.syms.items())
dem={}
def nm(a):
    i=bisect.bisect_right(st,(a,'\xff'))-1
    n=st[i][1]
    if n not in dem: dem[n]=subprocess.run(['c++filt',n],capture_output=True,text=True).stdout.strip()
    return dem[n]+('+'+hex(a-st[i][0]) if a!=st[i][0] else '')
name=sys.argv[1]
cands=[n for n in e.syms if name in subprocess.run(['c++filt',n],capture_output=True,text=True).stdout] if not name.startswith('_Z') else [name]
a=e.syms[cands[0]]; end=st[bisect.bisect_right(st,(a,'\xff'))][0]
print(nm(a), hex(a), hex(end-a))
for i in md.disasm(bytes(e.uc.mem_read(a,min(end-a,int(os.environ.get('LEN','4000'))))),a):
    t=''
    if i.mnemonic=='call' and i.op_str.startswith('0x'): t=nm(int(i.op_str,16))
    print(hex(i.address), i.mnemonic, i.op_str, t)
