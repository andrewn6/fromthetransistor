`timescale 1ns / 1ps 

module mmu (
    input wire clk,
    input wire reset,

    input wire [31:0] va,
    input wire [31:0] cpu_wdata,
    input wire cpu_we,
    input wire cpu_byte,
    input wire mem_req,
    input wire user,   // cpu has no modes yet, soc ties this low
    output wire [31:0] cpu_rdata,
    output wire stall,
    output wire fault,

    output wire [31:0] pa,
    output wire [31:0] mem_wdata,
    output wire mem_we,
    output wire mem_byte,
    input wire [31:0] mem_rdata
);
    localparam PAGE_BITS = 12; // 4KB pages 

    wire [19:0] vpn = va[31:PAGE_BITS];
    wire [11:0] page_off = va[PAGE_BITS-1:0];

    // ctrl window (0x40006000, mmio, never translated):
    // +0 CTRL bit0 enable, +4 TTBR (16KB aligned),
    // +8 FAR fault va, +C FSR fault cause (both read-only).
    // any write to either flushes the tlb.
    reg en;
    reg [31:0] ttbr;
    reg [31:0] far;
    reg [2:0] fsr;

    wire is_mmio  = (va[31:28] == 4'h4);
    wire ctrl_hit = (va[31:4] == 28'h4000600);
    wire ctrl_wr  = ctrl_hit && cpu_we;

    always @(posedge clk or posedge reset) begin
        if (reset) begin
            en <= 1'b0;
            ttbr <= 32'd0;
        end else if (ctrl_wr) begin
            if (va[3:2] == 2'd0) en <= cpu_wdata[0];
            if (va[3:2] == 2'd1) ttbr <= {cpu_wdata[31:14], 14'd0};
        end
    end

    wire [31:0] ctrl_rdata = (va[3:2] == 2'd0) ? {31'd0, en} :
                             (va[3:2] == 2'd1) ? ttbr :
                             (va[3:2] == 2'd2) ? far : {29'd0, fsr};

    // TLB 
    // 32 entries, mapped directly, vpn[4:0] picks the slot 
    localparam TLB_ENTRIES = 32;

    reg tlb_valid[0:TLB_ENTRIES-1];
    reg [19:0] tlb_tag [0:TLB_ENTRIES-1];
    reg [31:0] tlb_pte [0:TLB_ENTRIES-1];

    wire [4:0] tlb_index = vpn[4:0];
    wire tlb_hit = tlb_valid[tlb_index] && (tlb_tag[tlb_index] == vpn);
    wire [31:0] tlb_pte_out = tlb_pte[tlb_index];

    // only real cpu accesses outside mmio get translated
    wire translate = en && mem_req && !is_mmio;
    wire in_range  = (va[31:24] == 8'd0);

    // walk: memory is combinational, so a miss costs one cycle.
    // the bus reads the pte, the cpu is frozen, the edge fills the tlb.
    wire walk_now = translate && in_range && !tlb_hit;
    // table is 4096 ptes (16KB), so only va[23:12] indexes it.
    wire [31:0] pte_addr = {ttbr[31:14], va[23:12], 2'b00};

    // faults are decided on the hit, so a missing pte faults
    // instead of walking forever.
    wire range_fault = translate && !in_range;
    wire perm_fault  = translate && in_range && tlb_hit &&
                       (!tlb_pte_out[0] ||               // not present
                        (cpu_we && !tlb_pte_out[1]) ||   // read-only
                        (user && !tlb_pte_out[2]));      // kernel only
    wire [2:0] fault_code = range_fault ? 3'd1 :
                            !tlb_pte_out[0] ? 3'd2 :
                            (cpu_we && !tlb_pte_out[1]) ? 3'd3 : 3'd4;
    
    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
            far <= 32'd0;
            fsr <= 3'd0;
        end else if (fault) begin 
            far <= va;
            fsr <= fault_code;
        end 
    end

    // sequential reset and flush drop every valid bit.
    integer i;
    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
            for (i = 0; i < TLB_ENTRIES; i = i + 1)
                tlb_valid[i] <= 1'b0;
        end else if (ctrl_wr) begin
            for (i = 0; i < TLB_ENTRIES; i = i + 1)
                tlb_valid[i] <= 1'b0;
        end else if (walk_now) begin
            tlb_valid[tlb_index] <= 1'b1;
            tlb_tag[tlb_index] <= vpn;
            tlb_pte[tlb_index] <= mem_rdata;
        end 
    end

    wire [31:0] pa_translated = {tlb_pte_out[31:12], page_off};

    assign fault     = range_fault || perm_fault;
    assign stall     = walk_now;
    assign pa        = (!en || is_mmio) ? va :
                       (walk_now || fault) ? pte_addr : pa_translated;
    assign mem_wdata = cpu_wdata;
    assign mem_we    = cpu_we && !walk_now && !fault && !ctrl_hit;
    assign mem_byte  = cpu_byte && !walk_now;
    assign cpu_rdata = ctrl_hit ? ctrl_rdata : mem_rdata;

endmodule
