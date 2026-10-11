.section .text 
.global _start

.equ UART_BASE, 0x40001000
.equ UART_STATUS, UART_BASE + 0x4 
.equ UART_DATA, UART_BASE + 0x08
.equ RAM_BASE,  0x80000000 0
.equ DATA_READY, 0x01 
.equ END_OF_CODE, 0xFF 

.start:
  bl uart_init 

  ldr r1 =RAM_BASE

receive_loop:
  bl uart_receive_byte 
  strb r0, [r1]
  add r1, r1, #1
  cmp r0, #END_OF_CODE
  bne receive_loop

  ldr r0, =RAM_BASE
  bx r0

uart_init:
    mov r0, #0x03
    str r0, [UART_BASE]
    bx lr 

uart_receive_byte:
wait_for_data:
    ldr r0, [UART_STATUS]
    tst r0, #DATA_READY
    beq wait_for_data

    ldrb r0, [UART_DATA]
    bx lr
