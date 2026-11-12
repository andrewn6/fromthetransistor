@ hand-written assembly: no compiler and no runtime, so this file has to
@ define _start itself. it is assembled and linked exactly like build.py
@ does, then runs on the cpu model.
.text
.global _start
_start:
    ldr r1, =msg
    mov r0, #4          @ SYS_WRITE0: print the string at r1
    svc 0x123456
    mov r2, #0          @ r2 = 5 + 4 + 3 + 2 + 1
    mov r3, #5
loop:
    add r2, r2, r3
    sub r3, r3, #1
    cmp r3, #0
    bne loop
    mov r0, #0x18       @ SYS_EXIT
    ldr r1, =0x20026
    svc 0x123456
.pool

.data
msg: .asciz "hello from hand-written asm\n"
