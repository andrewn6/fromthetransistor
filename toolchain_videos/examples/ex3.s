.data
.global g
g:
    .word 42
.global msg
msg:
    .asciz "hello"
.Lstr0:
    .asciz "hello"
.Lstr1:
    .asciz "hi\n"

.text
.global main
main:
    push {fp, lr}
    mov fp, sp
    sub sp, sp, #16
    ldr r0, =.Lstr1
    push {r0}
    pop {r0}
    bl putstr
    add r0, fp, #-12
    push {r0}
    mov r0, #0
    mov r1, #4
    mul r0, r0, r1
    pop {r1}
    add r0, r1, r0
    push {r0}
    mov r0, #7
    pop {r1}
    str r0, [r1]
    add r0, fp, #-12
    push {r0}
    mov r0, #1
    mov r1, #4
    mul r0, r0, r1
    pop {r1}
    add r0, r1, r0
    push {r0}
    ldr r0, =g
    ldr r0, [r0]
    push {r0}
    add r0, fp, #-12
    push {r0}
    mov r0, #0
    mov r1, #4
    mul r0, r0, r1
    pop {r1}
    add r0, r1, r0
    ldr r0, [r0]
    pop {r1}
    add r0, r1, r0
    pop {r1}
    str r0, [r1]
    ldr r0, =msg
    push {r0}
    pop {r0}
    bl putstr
    add r0, fp, #-12
    push {r0}
    mov r0, #1
    mov r1, #4
    mul r0, r0, r1
    pop {r1}
    add r0, r1, r0
    ldr r0, [r0]
    push {r0}
    mov r0, #2
    pop {r1}
    mov r2, r0
    mov r0, r1
    mov r1, r2
    bl __divsi3
    b .Lret1
.Lret1:
    mov sp, fp
    pop {fp, lr}
    bx lr

