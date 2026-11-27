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
    ldr sp, =0x00800000    @ set up stack: qemu -kernel boots with sp=0
    bl __vectors
    bl main
    mov r0, #0x18
    ldr r1, =0x20026
    svc 0x123456
.Lhang:
    b .Lhang

@ trap vectors at 0x10008 (svc), 0x1000C (prefetch), 0x10010 (data).
@ each is ldr pc, [pc, #0x18], jumping through a word 0x20 further on.
__vectors:
    ldr r0, =0x00010008
    ldr r1, =0xE59FF018
    str r1, [r0]
    str r1, [r0, #4]
    str r1, [r0, #8]
    ldr r1, =__svc
    str r1, [r0, #0x20]
    ldr r1, =__pabt
    str r1, [r0, #0x24]
    ldr r1, =__dabt
    str r1, [r0, #0x28]
    bx lr

@ int syscall(int n, int a, int b, int c) - svc 0 with args in r0-r3
syscall:
    svc 0
    bx lr

@ svc stub: call the c handler at SVC_FN (0x10038) with r0-r3 as
@ the caller passed them, hand its r0 back. no handler: return -1.
@ movs pc, lr returns and restores the flags.
__svc:
    push {r1-r12, lr}
    ldr r12, =0x00010038
    ldr r12, [r12]
    cmp r12, #0
    beq .Lnosvc
    mov lr, pc
    bx r12
    b .Lsvcret
.Lnosvc:
    mvn r0, #0
.Lsvcret:
    pop {r1-r12, lr}
    .word 0xE1B0F00E       @ movs pc, lr

    
@ abort stubs: save what the faulting code can see, call the c
@ handler at ABORT_FN (0x10034) with r0 = 1 data / 0 prefetch,
@ restore, retry the access. subs pc also restores the flags.
__dabt:
    push {r0-r12, lr}
    mov r0, #1
    mov r1, sp             @ the saved frame, frame[13] is the return lr
    bl __abort_call
    pop {r0-r12, lr}
    .word 0xE25EF008       @ subs pc, lr, #8
__pabt:
    push {r0-r12, lr}
    mov r0, #0
    mov r1, sp
    bl __abort_call
    pop {r0-r12, lr}
    .word 0xE25EF004       @ subs pc, lr, #4
__abort_call:
    ldr r2, =0x00010034
    ldr r2, [r2]
    cmp r2, #0
    beq .Lnohandler
    bx r2                  @ tail call, handler returns to our caller
.Lnohandler:
    mov r0, #0x18          @ no handler: exit, like svc
    ldr r1, =0x20026
    svc 0x123456

    
@ int get_cpsr() / int get_spsr() / void set_spsr(int psr)
get_cpsr:
    .word 0xE10F0000       @ mrs r0, cpsr
    bx lr
get_spsr:
    .word 0xE14F0000       @ mrs r0, spsr
    bx lr
set_spsr:
    .word 0xE16FF000       @ msr spsr_fsxc, r0
    bx lr

@ void enter_mode(int pc, int sp, int psr) - exception-return to pc with
@ the mode and flags in psr, e.g. MODE_USR to drop into user mode. sp
@ is not banked yet, so this sp is the only sp from here on.
enter_mode:
    mov lr, r0
    mov sp, r1
    .word 0xE16FF002       @ msr spsr_fsxc, r2
    .word 0xE1B0F00E       @ movs pc, lr

@ void halt() - stop the sim from anywhere, like returning from main
halt:
    mov r0, #0x18
    ldr r1, =0x20026
    svc 0x123456

@ void jump_to(int addr) - tail-call an address, for the bootloader.
@ args live in r0, so this is just a bx with a veneer so C can call it
jump_to:
    bx r0

@ void putstr(char *s) - semihosting SYS_WRITE0
putstr:
    mov r1, r0
    mov r0, #4
    svc 0x123456
    bx lr

@ int putchar(int c) - semihosting SYS_WRITEC (r1 = pointer to the char)
putchar:
    sub sp, sp, #4
    strb r0, [sp]
    mov r1, sp
    mov r0, #3
    svc 0x123456
    add sp, sp, #4
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
    lsl r5, r1, #1      @ next doubled divisor
    cmp r5, r1
    blo .Ludsub         @ wrapped past 2^32: stop before shifting
    mov r1, r5
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
    push {r12, lr}
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
    pop {r12, lr}
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

BUILD_DIR = os.path.join(HERE, "build")

def build(cfile, elffile):
    with open(cfile) as f:
        src = f.read()
    asm_text = compiler.compile_text(src, cfile)
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

    # all outputs land in section4/build/ so the tree stays clean
    os.makedirs(BUILD_DIR, exist_ok=True)
    elfpath = os.path.join(BUILD_DIR, elffile)
    ld.write_elf(elfpath)
    print(f"built {elfpath} ({os.path.getsize(elfpath)} bytes)")
    print(f"run: qemu-system-arm -M versatilepb -nographic -semihosting -kernel {elfpath}")

    # flat image for the verilator arm7_cpu (section3): .text + .data
    # concatenated, to be loaded at the linked base. the cpu testbench
    # takes the file and the base as its two arguments.
    flat = b"".join(bytes(s.content) for s in ld.sections)
    binfile = os.path.join(BUILD_DIR, os.path.splitext(elffile)[0] + ".bin")
    with open(binfile, "wb") as f:
        f.write(flat)
    print(f"flat {binfile} ({len(flat)} bytes), load base {ld.sections[0].addr:#x}")
    print(f"cpu: ../section3/obj_dir_cpu/cpu_sim {binfile} {ld.sections[0].addr:#x}")

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: uv run python build.py input.c output.elf", file=sys.stderr)
        sys.exit(1)
    build(sys.argv[1], sys.argv[2])