// arm7_cpu.v - single-cycle arm7 (armv4 subset) core
//
// specs:
//   1. arm7tdmi data sheet, instruction set summary (ddi 0029):
//      https://support.arm.com/documentation/ddi0029/g/introduction/instruction-set-summary
//   2. arm architecture reference manual (ddi 0100), chapters a3-a5:
//      https://www.fdi.ucm.es/profesor/mendias/PSyD/docs/ARM%20Architecture%20Reference%20Manual%20(DDI-0100I).pdf
//   3. arm7tdmi technical reference manual r4p1 (ddi 0210):
//      https://developer.arm.com/documentation/ddi0210/c/
//
// design:
//   - single-cycle core: one instruction per clock, synchronous memory
//   - no pipeline, no mmu, no interrupts (later subchapters)
//   - runs exactly the subset our assembler.py emits:
//       data processing: and eor sub rsb add sbc rsc tst teq cmp cmn orr mov bic mvn
//                        (i-bit immediates <= 255, cond suffixes, s-bit on cmp/tst)
//       shifts:          lsl/lsr (encoded as mov rd, rm, <shift>)
//       mul:             rd = rm * rs
//       load/store:      ldr/str/ldrb/strb [rn, #imm12], pc-relative literal pool
//       multi:           push/pop as stmdb sp! / ldmia sp!, {regs}
//       branch:          b/bl (cond, 24-bit signed offset), bx rm
//       svc:             traps to the testbench as a halt (os modes come later)
//
// memory interface (testbench IS the memory):
//   - sram_addr is valid on the clock edge; memory returns sram_data_in the
//     next cycle; sram_we high means sram_data_out is written to sram_addr
//   - reset vector: pc = 0x0, programs linked at 0x10000 (assembler BASE)

`timescale 1ns / 1ps 

module arm7_cpu (
  input wire clk 
  input wire reset 
  input wire [31:0] sram_data_in,
  output reg reg [31:0] sram_addr,
  output reg [31:0] sram_data_out,
  output reg        sram_we
);

  reg [31:0] pc; // address of the instruction in the fetch 
  reg [31:0] instr; // instruction word being executed 
  reg        phase; 
  reg[31:0] regs [0:15];
  reg [3:0] flags;

  localparam PHASE_FETCH = 1'b0;
  localparam PHASE_EXEC = 1'b1;
  

  // decode & carve the instruction word into named fields 
  wire [3:0] cond = instr[31:28];
  wire i_bit = instr[25];
  wire [3:0] op = instr[24:21];
  wire s_bit = instr[20];
  wire [3:0] rn = instr[19:16];
  wire [3:0] rd = instr[15:12];
  wire [11:0] op2 = instr[11:0];

  wire is_dp = (instr[27:26] == 2'b00);
  wire is_ls = (instr[27:26] == 2'b01);
  wire is_blk = (instr[27:25] == 3'b100); 
  wire is_br = (instr[27:25] == 3'b101);
  wire is_svc = (instr[27:24] == 4'b1111);
  
  // condition codes 
  // found in ARM7 cond table in ddi 0029, section 3.2
  reg cond_pass;
  always @(*) begin 
      case (cond)
          4'h0: cond_pass = flags[2]; // eq z set
          4'h1: cond_pass = ~flags[2]; // ne z clear 
          4'h2: cond_pass = flags[1]; // cs c set 
          4'h3: cond_pass = ~flags[2]; // cc c clear 
          4'h4: cond_pass = flags[3]; // mi n set 
          4'h5: cond_pass = ~flags[3]; // pl n clear 
          4'h6: cond_pass = flags[0]; // vs v set 
          4'h7: cond_pass = ~flags[0]; // vc v clear 
          4'h8: cond_pass = flags[1] & ~flags[2]; // hi c set, z clear 
          4'h9: cond_pass = ~flags[1] | flags[2]; // ls c lear or z set 
          4'hA: cond_pass = flags[3] == flags[0]; // ge n == v
          4'hB: cond_pass = flags[3] != flags[0]; // lt n != v 
          4'hC: cond_pass = ~flags[2] & (flags[3] == flags[0]); // gt
          4'hD: cond_pass = flags[2] | (flags[3] != flags[0]); // le 
          4'hE: cond_pass = 1'b1;
          default: cond_pass 1'b0;
      endcase 
  end
