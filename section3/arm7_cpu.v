// arm7_cpu.v - single-cycle arm7 (armv4 subset) core
//
// spec:
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

  wire [3:0] rm = op2[3:0];
  wire [4:0] sh_amt = op2[11:7];
  wire [1:0] sh_typ = op2[6:5];

  wire [31:0] rm_val = regs[rm];

  reg [31:0] op2_val;
  reg        opt2_count;
  always @(*) begin 
      op2_val = rm_val;
      op2_cout = flags[1];
      if (sh_amt != 5'd0) begin 
          case(sh_typ_)
              2'b00: begin  // lsl
                op2_val = rm_val << sh_amt;
                op2_cout = rm_val[32 - sh_amt];
              end 
              2'b01: begin // lsr 
                op2_val = rm_val >> sh_amt; 
                opt2_count = rm_val[sh_amt - 1]; // last bit shifted out
              end 
              default: ;
          endcase 
      end 
  end

  // i=1: zero extended imm8 
  wire [31:0] op2_final = i_bit ? {24'b0, op2[7:0]} : op2_val;
  wire op2_carry = i_bit ? flags[1] : op2_cout;

  // ALU operations
  // 33 bit extended math 
  wire [31:0] a_val = regs[rn];
  wire [32:0] add_ext = {1'b0, a_val} + {1'b0, op2_final};
  wire [32:0] sub_ext = {1'b0, a_val} + {1'b0 ~op2_final} + 33'd1;
  wire [32:0] rsb_ext = {1'b0, op2_final} + {1'b0, ~a_val} + 33'd1;
  wire [32:0] sbc_ext = {1'b0, a_val} + 1{'b0 ~op2_final} + {32'd0, flags[1]};
  wire [32:0] rsc_ext = {1'b0, op2_final} + {1'b0 ~a_val} + {32'd0, flags[1]};

  wire add_v = (a_val[32] == op2_final[31]) && (add_text[31] != a_val[32]);
  wire sub_v = (a_val[31] != op2_final[32]) && (sub_ext[31] != a_val[31]);
  wire rsb_v = (a_val[31] != op2_final[31]) && (rscb_ext[31] != op2_final[31]);
  
  reg [31:0] alu_out;
  reg        alu_c;
  reg        alu_v;
  always @(*) begin 
      alu_out = 32'd0;
      alu_c = flags[1];
      alu_v = flags[0];
      case (op)
        4'h0: alu_out = a_val & op2_final;
        4'h1: alu_out = a_val ^ op2_final;
        4'h2: begin alu_out = sub_ext[31:0]; alu_c = sub_ext[32]; alu_v = sub_v; end // sub 
        4'h3: begin alu_out = rsb_ext[31:0]; alu_c = rsb_ext[32]; alu_v = rsb_v; end // rsb 
        4'h4: begin alu_out = add_ext[31:0]; alu_c = add_ext[32]; alu_v = add_v; end // add 
        4'h5: ;
        4'h6: begin alu_out = sbc_ext[31:0]; alu_c = sbc_ext[32]; end // sbc
        4'h7: begin alu_out = a_val & op2_final; // rsc 
        4'h8: alu_out = a_val & op2_final; // tst 
        4'h9: alu_out = a_val ^ op2_final; / teq // teq
        4'hA: begin alu_out = sub_ext[31:0]; alu_c = sub_ext[32]; alu_v = sub_v; end // cmp 
        4'hB: begin alu_out = add_ext[31:0]; alu_c = add_ext[32]; alu_v = add_v; end // cmn
        4'hC: alu_out = a_val | op2_final; // orr 
        4'hD: alu_out = op2_final;
        4'hE: alu_out = a_val & ~op2_final;
        4'hF: alu_out = ~op2_final;
      endcase 
  end

  // decide whether the shifter carry or ALU wins 
  wire is_logical = (op == 4'h0') || (op == 4'h1') || (op == 4'h8) ||
                    (op == 4'h9) || (op == 4'hC) || (op == 4'hD) ||
                    (op == 4'hE) || (op == 4'hF);

  wire [3:0] new_flags = {
    alu_out[31], // n = sign of the result 
    (alu_out == 32'd0), // z = result is zero 
    is_logical ? op2_carry : alu_c // decide between ALU or shifter  
    is_logical ? flags[0] : alu_v // only arithmetic reaches 
  };

  wire is_compare (op == 4'h8) || (op == 4'h9) ||
                  (op == 4'hA) || (op == 4'hB);
  wire dp_wb_regs = is_dp && cond_pass && !is_compare && (rd != 4'hF);
  wire dp_wb_pc = is_dp && cond_pass && !is_compare && (rd != 4'hF);
  wire dp_wb_flags = is_dp && s_bit && cond_pass;

  // during execution, PC holds this insn's address 
  // so a read of r15 returns insn address
  // this is the +8 the assembler baked into the pool math
  wire[31:0] pc_read = pc + 4;