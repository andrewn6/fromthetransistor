`timescale 1ns / 1ps 

module mmu (
    input wire clk,
    input wire reset,

    input wire en,
    input wire [31:0] ttbr,
    input wire tlb_fill,
    input wire [19:0] tlb_fill_vpn,
    input wire [31:0] tlb_fill_pte,
    input wire tlb_flush,

    input wire [31:0] va,
    input wire [31:0] cpu_wdata,
    input wire cpu_we,
    input wire cpu_byte,
    output wire [31:0] cpu_rdata,
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

    // TLB 
    // 32 entries, mapped directly, vpn[4:0] picks the slot 
    localparam TLB_ENTRIES = 32;

    reg tlb_valid[0:TLB_ENTRIES-1];
    reg [19:0] tlb_tag [0:TLB_ENTRIES-1];
    reg [31:0] tlb_pte [0:TLB_ENTRIES-1];

    wire [4:0] tlb_index = vpn[4:0];
    wire tlb_hit = tlb_valid[tlb_index] && (tlb_tag[tlb_index] == vpn);
    wire [31:0] tlb_pte_out = tlb_pte[tlb_index];

    // sequential reset and flush drop every valid bit.
    integer i;
    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
            for (i = 0; i < TLB_ENTRIES; i = i + 1)
                tlb_valid[i] <= 1'b0;
        end else if (tlb_flush) begin 
            for (i = 0; i < TLB_ENTRIES; i = i + 1)
                tlb_valid[i] <= 1'b0;
        end else if (tlb_fill) begin
            tlb_valid[tlb_fill_vpn[4:0]] <= 1'b1;
            tlb_tag[tlb_fill_vpn[4:0]] <= tlb_fill_vpn;
            tlb_pte[tlb_fill_vpn[4:0]] <= tlb_fill_pte;
        end 
    end

    wire [31:0] pa_translated = {tlb_pte_out[31:12], page_off};

    assign pa = !en ? va :
                tlb_hit ? pa_translated : va; // TEMP 
    assign mem_wdata = cpu_wdata;
    assign mem_we    = cpu_we;
    assign mem_byte  = cpu_byte;
    assign cpu_rdata = mem_rdata;
    assign fault     = 1'b0;

endmodule
