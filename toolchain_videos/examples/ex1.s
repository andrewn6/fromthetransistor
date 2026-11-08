.data

.text
.global main
main:
    push {fp, lr}
    mov fp, sp
    mov r0, #2
    push {r0}
    mov r0, #3
    push {r0}
    mov r0, #4
    pop {r1}
    mul r0, r1, r0
    pop {r1}
    add r0, r1, r0
    b .Lret1
.Lret1:
    mov sp, fp
    pop {fp, lr}
    bx lr

