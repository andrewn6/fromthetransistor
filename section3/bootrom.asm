@ the serial bootloader, baked into the image of the fpga.
@ protocol: 3-byte big-endian length, then that many bytes.
@ the program arrives over the uart and lands at 0x20000.
.text 
.global _start


_start:
  @ r4 = uart window: DATA at +0, STATUS at +4 (matches soc.v)
  ldr r4, = 0x40001000
  ldr r6, = 0x00020000

  @ read 3-byte big-endian length into r7
  bl rx_byte
  mov r7, r0 
  bl rx_byte 
  lsl r0, r0, #8
  orr r7, r7, r0 
  bl rx_byte
  lsl r0, r0, #24
  orr r7, r7, r0 

@ store bytes to [r6], counting r7 down to zero
recv_loop:
  cmp r7, #0
  beq done 
  bl rx_byte
  strb r0, [r6]
  add r6, r6, #1 
  sub r7, r7, #1
  b recv_loop 

@ jump to what we received
done:
  ldr r0, = 0x00020000
  bx r0

@ rx_byte: spin until the uart has a byte
rx_byte:
  ldr r1, [r4, #4]     @ STATUS: bit0 = rx_valid
  tst r1, #1 
  beq rx_byte
  ldrb r0, [r4, #0]    @ DATA: reading it clears rx_valid
  bx lr