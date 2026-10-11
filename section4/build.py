import sys, os

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "section3"))

import compiler
import assembler
import linker

RUNTIME = r"""
.text
.global _start
_start:
    bl main
    mov r0, #0x18
    ldr r1, =0x20026
    svc 0x123456
.Lhang:
    b .Lhang

@ void putstr(char *s) - semihosting SYS_WRITE0
putstr:
    mov r1, r0
    mov r0, #4
    svc 0x123456
    bx lr

@ unsigned divide: r0 = r0 / r1, remainder -> r2
__udivsi3:
    mov r2, #0
    mov r3, #0
    mov r4, #1
    cmp r1, #0
    beq .Ludzero
.Ludshift:
    cmp r1, r0
    bhi .Ludsub
    lsl r1, r1, #1
    lsl r4, r4, #1
    b .Ludshift
.Ludsub:
    cmp r0, r1
    blo .Ludnext
    sub r0, r0, r1
    add r3, r3, r4
.Ludnext:
    lsr r1, r1, #1
    lsr r4, r4, #1
    cmp r4, #0
    bne .Ludsub
    mov r2, r0
    mov r0, r3
    bx lr
.Ludzero:
    mov r0, #0
    mov r2, #0
    bx lr

@ signed: r0 = r0 / r1
__divsi3:
    cmp r0, #0
    rsblt r0, r0, #0
    movlt r12, #1
    movge r12, #0
    cmp r1, #0
    rsblt r1, r1, #0
    eorlt r12, r12, #1
    bl __udivsi3
    cmp r12, #0
    rsbne r0, r0, #0
    bx lr

@ signed: r0 = r0 % r1 (sign follows dividend)
__modsi3:
    push {r12, lr}
    cmp r0, #0
    movlt r12, #1
    movge r12, #0
    rsblt r0, r0, #0
    cmp r1, #0
    rsblt r1, r1, #0
    bl __udivsi3
    cmp r12, #0
    rsbne r2, r2, #0
    mov r0, r2
    pop {r12, lr}
    bx lr
"""

def build(cfile, elffile):
    with open(cfile) as f:
        src = f.read()
    asm_text = compiler.compile_text(src)
    full = RUNTIME + "\n" + asm_text

    a = assembler.Asm()
    a.pass1(full)
    a.pass2()
    text, data, symbols, relocs = a.object()

    ld = linker.ELFLinker()
    ld.add_section(".text", text)
    ld.add_section(".data", data, flags=linker.SHF_ALLOC | linker.SHF_WRITE)
    for name, (sec, off) in symbols.items():
        ld.add_symbol(name, off, sec,
                      type_=linker.STT_OBJECT if sec == ".data" else linker.STT_FUNC)
    for (sec, off, rtype, sym, addend) in relocs:
        ld.add_relocation(off, sym, sec, type_=rtype, addend=addend)
    ld.link(entry="_start")
    ld.write_elf(elffile)
    print(f"built {elffile} ({os.path.getsize(elffile)} bytes)")
    print(f"run: qemu-system-arm -M versatilepb -nographic -semihosting -kernel {elffile}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: uv run python build.py input.c output.elf", file=sys.stderr)
        sys.exit(1)
    build(sys.argv[1], sys.argv[2])