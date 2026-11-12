@ stand-in boot rom for the playground board. it does what
@ section3/bootrom.asm sets out to do: wait on the soc.v uart, receive an
@ image, jump to it. the board boots section3/bootrom.asm instead as soon as
@ that file assembles.
@
@ linked at 0xf0000 (top of sram, the "block ram" the bitstream fills in).
@ the cpu resets to 0x10000, where the board plants a branch to _start.
@
@ protocol, little endian: load address (4), length (4), entry (4), image
@ leds at 0x40000000: 1 = rom alive, 3 = receiving, 7 = jumping to the image

.text
.global _start

_start:
  ldr sp, =0x00800000
  ldr r4, =0x40001000
  ldr r5, =0x40000000
  mov r0, #1
  str r0, [r5, #0]
  ldr r0, =banner
  bl puts

  bl rx_word
  mov r6, r0
  bl rx_word
  mov r7, r0
  bl rx_word
  mov r8, r0
  mov r0, #3
  str r0, [r5, #0]

  mov r9, r6
copy:
  cmp r7, #0
  beq go
  bl rx_byte
  strb r0, [r9, #0]
  add r9, r9, #1
  sub r7, r7, #1
  b copy

go:
  ldr r0, =jumping
  bl puts
  mov r0, #7
  str r0, [r5, #0]
  bx r8

@ rx_byte: spin until STATUS.rx_valid, then read DATA (which clears it)
rx_byte:
  ldr r1, [r4, #4]
  tst r1, #2
  beq rx_byte
  ldrb r0, [r4, #0]
  bx lr

@ rx_word: four bytes, least significant first
rx_word:
  push {r10, lr}
  bl rx_byte
  mov r10, r0
  bl rx_byte
  lsl r0, r0, #8
  orr r10, r10, r0
  bl rx_byte
  lsl r0, r0, #16
  orr r10, r10, r0
  bl rx_byte
  lsl r0, r0, #24
  orr r10, r10, r0
  mov r0, r10
  pop {r10, lr}
  bx lr

@ puts: r0 = nul-terminated string, waits on STATUS.tx_ready per byte
puts:
  mov r2, r0
puts_next:
  ldrb r0, [r2, #0]
  cmp r0, #0
  beq puts_done
puts_wait:
  ldr r1, [r4, #4]
  tst r1, #1
  beq puts_wait
  strb r0, [r4, #0]
  add r2, r2, #1
  b puts_next
puts_done:
  bx lr

.data
banner:
  .asciz "rom: waiting for an image on the uart\n"
jumping:
  .asciz "rom: image received, jumping\n"
