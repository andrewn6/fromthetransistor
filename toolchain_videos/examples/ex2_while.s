.data

.text
.global sum
sum:
    push {fp, lr}
    mov fp, sp
    str r0, [fp, #-4]
    sub sp, sp, #16
    mov r0, #0
    str r0, [fp, #-8]
    mov r0, #0
    str r0, [fp, #-12]
.Lwhile2:
    ldr r0, [fp, #-12]
    push {r0}
    ldr r0, [fp, #-4]
    pop {r1}
    cmp r1, r0
    mov r0, #0
    movlt r0, #1
    cmp r0, #0
    beq .Lwend3
    add r0, fp, #-8
    push {r0}
    ldr r0, [r0]
    push {r0}
    ldr r0, [fp, #-12]
    pop {r1}
    add r0, r1, r0
    pop {r1}
    str r0, [r1]
    add r0, fp, #-12
    push {r0}
    ldr r0, [r0]
    push {r0}
    mov r0, #1
    pop {r1}
    add r0, r1, r0
    pop {r1}
    str r0, [r1]
    b .Lwhile2
.Lwend3:
    ldr r0, [fp, #-8]
    b .Lret1
.Lret1:
    mov sp, fp
    pop {fp, lr}
    bx lr

