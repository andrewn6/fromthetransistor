.data

.text
.global abs
abs:
    push {fp, lr}
    mov fp, sp
    str r0, [fp, #-4]
    sub sp, sp, #8
    ldr r0, [fp, #-4]
    push {r0}
    mov r0, #0
    pop {r1}
    cmp r1, r0
    mov r0, #0
    movlt r0, #1
    cmp r0, #0
    beq .Lelse2
    ldr r0, [fp, #-4]
    rsb r0, r0, #0
    b .Lret1
.Lelse2:
    ldr r0, [fp, #-4]
    b .Lret1
.Lret1:
    mov sp, fp
    pop {fp, lr}
    bx lr

