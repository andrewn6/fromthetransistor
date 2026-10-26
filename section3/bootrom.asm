@ bootrom.asm - downloads a program over serial into ram, then jumps to it
@
@ does not assemble yet. fix before the 04-bootrom chapter:
@   1. assembler.py: no .section directive -> use .text (or add support)
@   2. assembler.py: no .equ constants -> add support, or inline the values
@   3. symbolic immediates (#END_OF_CODE, #DATA_READY) fail int() parsing
@   4. absolute mmio bases ([UART_STATUS] etc) have no arm encoding ->
@      rewrite as: ldr r1, =UART_STATUS / ldr r0, [r1] (see compiler.py)
@   5. line "RAM_BASE, 0x80000000 0" has a stray trailing 0
@   6. RAM_BASE (0x80000000) != toolchain load address (0x10000) ->
@      downloaded code is linked elsewhere, the jump would land nowhere
@   7. bootrom must be linked at the reset vector (0x0), but assembler.py
@      hardcodes BASE = 0x10000 for everything

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
