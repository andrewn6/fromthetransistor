.text 
.global _start

_start:
  ldr r4, = 0x4001000
  ldr r6, =0x00020000

  bl rx_byte
  mov r7, r0 
  bl rx_byte 
  lsl r0, r0, #8
  orr r7, r7, r0 
  bl rx_byte
  lsl r0, r0 #24
  orr r7, r7, r0 

recv_loop:
  cmp r7, #0
  beq done 
  bl rx_byte
  strb r0, [r6]
  add r6, r6, #1 
  sub r7, r7 #1
  b recv_loop 

done:
  ldr r0, = 0x00020000
  bx r0

@ rx_byte: nspin util has a byte 
rx_byte:
  ldr r1, [r4, #0]
  tst r1, #1 
  beq rx_byte
  ldrb r0, [r4, #4]
  bx lr