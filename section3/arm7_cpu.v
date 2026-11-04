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
//   - fetch/execute and block-transfer phases, one memory access per clock
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
  input  wire        clk,
  input  wire        reset,
  input wire [31:0] sram_data_in,
  output reg  [31:0] sram_addr,
  output reg [31:0] sram_data_out,
  output reg         sram_we,
  output reg         sram_byte,   // 1 = byte access (ldrb/strb)
  output reg         halted       // svc trap, testbench stops on it
);

  reg [31:0] pc; // address of the instruction in the fetch 
  reg [31:0] instr; // instruction word being executed 
  reg [1:0]  phase;
  reg[31:0] regs [0:15];
  reg [3:0] flags;

  localparam PHASE_FETCH = 2'b00;
  localparam PHASE_EXEC = 2'b01;
  localparam PHASE_BLOCK = 2'b10;

  integer i;
  

  // decode & carve the instruction word into named fields 
  wire [3:0] cond = instr[31:28];
  wire i_bit = instr[25];
  wire [3:0] op = instr[24:21];
  wire s_bit = instr[20];
  wire [3:0] rn = instr[19:16];
  wire [3:0] rd = instr[15:12];
  wire [11:0] op2 = instr[11:0];
  wire [31:0] pc_read = pc + 4;

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
          4'h3: cond_pass = ~flags[1]; // cc c clear
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
          default: cond_pass = 1'b0;
      endcase
  end

  wire [3:0] rm = op2[3:0];
  wire [4:0] sh_amt = op2[11:7];
  wire [1:0] sh_typ = op2[6:5];

  wire [31:0] rm_val = (rm == 4'hF) ? pc_read : regs[rm];

  reg [31:0] op2_val;
  reg        op2_cout;
  always @(*) begin 
      op2_val = rm_val;
      op2_cout = flags[1];
      case (sh_typ)
          2'b00: begin // lsl
            if (sh_amt != 5'd0) begin
              op2_val = rm_val << sh_amt;
              op2_cout = rm_val[32 - sh_amt];
            end
          end
          2'b01: begin // lsr
            if (sh_amt == 5'd0) begin
              op2_val = 32'd0;
              op2_cout = rm_val[31];
            end else begin
              op2_val = rm_val >> sh_amt;
              op2_cout = rm_val[sh_amt - 1];
            end
          end
          default: ;
      endcase
  end

  // i=1: zero extended imm8 
  wire [31:0] op2_final = i_bit ? {24'b0, op2[7:0]} : op2_val;
  wire op2_carry = i_bit ? flags[1] : op2_cout;

  // ALU operations
  // 33 bit extended math 
  wire [31:0] a_val = (rn == 4'hF) ? pc_read : regs[rn];
  wire [32:0] add_ext = {1'b0, a_val} + {1'b0, op2_final};
  wire [32:0] sub_ext = {1'b0, a_val} + {1'b0, ~op2_final} + 33'd1;
  wire [32:0] rsb_ext = {1'b0, op2_final} + {1'b0, ~a_val} + 33'd1;
  wire [32:0] sbc_ext = {1'b0, a_val} + {1'b0, ~op2_final} + {32'd0, flags[1]};
  wire [32:0] rsc_ext = {1'b0, op2_final} + {1'b0, ~a_val} + {32'd0, flags[1]};

  wire add_v = (a_val[31] == op2_final[31]) && (add_ext[31] != a_val[31]);
  wire sub_v = (a_val[31] != op2_final[31]) && (sub_ext[31] != a_val[31]);
  wire rsb_v = (a_val[31] != op2_final[31]) && (rsb_ext[31] != op2_final[31]);
  wire sbc_v = (a_val[31] != op2_final[31]) && (sbc_ext[31] != a_val[31]);
  wire rsc_v = (a_val[31] != op2_final[31]) && (rsc_ext[31] != op2_final[31]);
  
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
        4'h6: begin alu_out = sbc_ext[31:0]; alu_c = sbc_ext[32]; alu_v = sbc_v; end // sbc
        4'h7: begin alu_out = rsc_ext[31:0]; alu_c = rsc_ext[32]; alu_v = rsc_v; end // rsc
        4'h8: alu_out = a_val & op2_final; // tst 
        4'h9: alu_out = a_val ^ op2_final; // teq
        4'hA: begin alu_out = sub_ext[31:0]; alu_c = sub_ext[32]; alu_v = sub_v; end // cmp 
        4'hB: begin alu_out = add_ext[31:0]; alu_c = add_ext[32]; alu_v = add_v; end // cmn
        4'hC: alu_out = a_val | op2_final; // orr 
        4'hD: alu_out = op2_final;
        4'hE: alu_out = a_val & ~op2_final;
        4'hF: alu_out = ~op2_final;
      endcase 
  end

  // decide whether the shifter carry or ALU wins 
  wire is_logical = (op == 4'h0) || (op == 4'h1) || (op == 4'h8) ||
                    (op == 4'h9) || (op == 4'hC) || (op == 4'hD) ||
                    (op == 4'hE) || (op == 4'hF);

  wire [3:0] new_flags = {
    alu_out[31], // n = sign of the result 
    (alu_out == 32'd0), // z = result is zero 
    is_logical ? op2_carry : alu_c, // decide between ALU or shifter  
    is_logical ? flags[0] : alu_v // only arithmetic reaches 
  };

  wire is_compare = (op == 4'h8) || (op == 4'h9) ||
                  (op == 4'hA) || (op == 4'hB);
  // during execution, PC holds this insn's address + 4
  // so a read of r15 returns insn address + 8
  // this is the +8 the assembler baked into the pool math
  wire ls_u = instr[23];
  wire ls_b = instr[22];
  wire ls_l = instr[20];
  wire [31:0] ls_off = {20'b0, instr[11:0]};
  wire [31:0] ls_base = (rn == 4'hF) ? pc_read : regs[rn];
  wire [31:0] ls_ea = ls_u ? (ls_base + ls_off)
                             : (ls_base - ls_off);

  // stores data: strb puts byte in the low lane
  wire [31:0] ls_src = (rd == 4'hF) ? (pc + 8) : regs[rd];
  wire [31:0] ls_wdata = ls_b ? {24'b0, ls_src[7:0]} : ls_src;
  wire [31:0] ls_rdata = ls_b ? {24'b0, sram_data_in[7:0]}
                              : sram_data_in;
  
  // a store never writes the reg file, a load writes rd 
  wire ls_wb = is_ls && cond_pass && ls_l;

  reg [15:0] blk_mask;
  reg [3:0] blk_idx;
  reg [31:0] blk_addr;
  reg        blk_found;
  integer    blk_scan;

  wire blk_l = instr[20];
  wire [15:0] blk_list = instr[15:0];

  wire [4:0] blk_n = {4'b0, instr[15]} + {4'b0, instr[14]} +
                     {4'b0, instr[13]} + {4'b0, instr[12]} +
                     {4'b0, instr[11]} + {4'b0, instr[10]} +
                     {4'b0, instr[9]} + {4'b0, instr[8]} +
                     {4'b0, instr[7]} + {4'b0, instr[6]} +
                     {4'b0, instr[5]} + {4'b0, instr[4]} +
                     {4'b0, instr[3]} + {4'b0, instr[2]} +
                     {4'b0, instr[1]} + {4'b0, instr[0]};
  wire [31:0] blk_bytes = {25'b0, blk_n, 2'b00};

  // first bus address: STMDB starts at sp - 4n, ldmia starts at sp
  wire [31:0] blk_start = blk_l ? regs[13]
                                : (regs[13] - blk_bytes);

  wire br_link = instr[24];
  wire [31:0] br_imm = {{6{instr[23]}}, instr[23:0], 2'b00}; // <<2
  wire [31:0] br_dest = pc + 4 + br_imm; // pc = insn+4, total is x2 thusfore 8 

  wire is_bx = is_dp && (instr[27:20] == 8'h12) &&
                (instr[19:12] == 8'hFF) && (instr[11:4] == 8'hF1);


  wire [31:0] bx_target = rm_val & 32'hFFFFFFFC;

  wire is_mul = is_dp && !i_bit && (instr[27:21] == 7'b0000000) &&
                (instr[15:12] == 4'b0000) && (instr[7:4] == 4'b1001);
  wire [3:0] rs_reg = op2[11:8];
  wire [31:0] rs_val = (rs_reg == 4'hF) ? pc_read : regs[rs_reg];
  wire [31:0] mul_rest = rm_val * rs_val; // low 32 bits
  wire [3:0] mul_rd = instr[19:16];
  wire [3:0] mul_flags = {mul_rest[31], (mul_rest == 32'd0), flags[1:0]};
  wire dp_wb_regs = is_dp && !is_mul && !is_bx && cond_pass && !is_compare && (rd != 4'hF);
  wire dp_wb_pc = is_dp && !is_mul && !is_bx && cond_pass && !is_compare && (rd == 4'hF);
  wire dp_wb_flags = is_dp && !is_mul && !is_bx && s_bit && cond_pass;

  always @(*) begin
    blk_idx = 4'd0;
    blk_found = 1'b0;
    for (blk_scan = 0; blk_scan < 16; blk_scan = blk_scan + 1) begin
      if (blk_mask[blk_scan] && !blk_found) begin
        blk_idx = blk_scan[3:0];
        blk_found = 1'b1;
      end
    end
  end

  always @(*) begin
    sram_addr = pc;
    sram_data_out = 32'd0;
    sram_we = 1'b0;
    sram_byte = 1'b0;

    if (!reset && !halted) begin
      case (phase)
        PHASE_EXEC: begin
          if (is_ls && cond_pass) begin
            sram_addr = ls_ea;
            sram_data_out = ls_wdata;
            sram_we = !ls_l;
            sram_byte = ls_b;
          end
        end
        PHASE_BLOCK: begin
          if (blk_mask != 16'd0) begin
            sram_addr = blk_addr;
            sram_we = !blk_l;
            if (blk_idx == 4'hF)
              sram_data_out = pc + 8;
            else
              sram_data_out = regs[blk_idx];
          end
        end
        default: ;
      endcase
    end
  end

  always @(posedge clk) begin
    if (reset) begin
      pc <= 32'h00010000;
      instr <= 32'd0;
      phase <= PHASE_FETCH;
      flags <= 4'd0;
      halted <= 1'b0;
      blk_mask <= 16'd0;
      blk_addr <= 32'd0;
      for (i = 0; i < 16; i = i + 1)
        regs[i] <= 32'd0;
    end else if (!halted) begin
      case (phase)
        PHASE_FETCH: begin
          instr <= sram_data_in;
          pc <= pc + 4;
          phase <= PHASE_EXEC;
        end
        PHASE_EXEC: begin
          phase <= PHASE_FETCH;
          if (cond_pass) begin
            if (is_svc) begin
              halted <= 1'b1;
            end else if (is_bx) begin
              pc <= bx_target;
            end else if (is_br) begin
              if (br_link)
                regs[14] <= pc;
              pc <= br_dest;
            end else if (is_blk) begin
              if (blk_list != 16'd0) begin
                blk_mask <= blk_list;
                blk_addr <= blk_start;
                phase <= PHASE_BLOCK;
              end
            end else if (is_ls) begin
              if (ls_wb) begin
                if (rd == 4'hF)
                  pc <= ls_rdata & 32'hFFFFFFFC;
                else
                  regs[rd] <= ls_rdata;
              end
            end else if (is_mul) begin
              regs[mul_rd] <= mul_rest;
              if (s_bit)
                flags <= mul_flags;
            end else if (is_dp) begin
              if (dp_wb_regs)
                regs[rd] <= alu_out;
              if (dp_wb_pc)
                pc <= alu_out & 32'hFFFFFFFC;
              if (dp_wb_flags)
                flags <= new_flags;
            end
          end
        end
        PHASE_BLOCK: begin
          if (blk_mask == 16'd0) begin
            phase <= PHASE_FETCH;
          end else begin
            if (blk_l) begin
              if (blk_idx == 4'hF)
                pc <= sram_data_in & 32'hFFFFFFFC;
              else
                regs[blk_idx] <= sram_data_in;
            end

            blk_mask <= blk_mask & ~(16'h0001 << blk_idx);
            blk_addr <= blk_addr + 4;

            if ((blk_mask & ~(16'h0001 << blk_idx)) == 16'd0) begin
              if (blk_l)
                regs[13] <= regs[13] + blk_bytes;
              else
                regs[13] <= regs[13] - blk_bytes;
              phase <= PHASE_FETCH;
            end
          end
        end
        default: phase <= PHASE_FETCH;
      endcase
    end
  end
endmodule
