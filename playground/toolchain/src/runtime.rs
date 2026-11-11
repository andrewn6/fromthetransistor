// the asm runtime that build.py prepends to every compiled program:
// _start (stack setup, call main, semihosting exit), putstr/putchar via
// semihosting, and the software divide helpers. copied verbatim from
// section4/build.py so the bytes match.
pub const RUNTIME: &str = r#"
.text
.global _start
_start:
    ldr sp, =0x00800000    @ set up stack: qemu -kernel boots with sp=0
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
"#;
