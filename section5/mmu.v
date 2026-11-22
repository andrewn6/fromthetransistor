`timescale 1ns / 1ps 

module mmu (
    input wire clk;
    input wire reset;

    input wire en,
    input wire [31:0] ttbr,

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

    assign pa = va;
    assign mem_wdata = cpu_wdata;
    assign mem_we = cpu_we;
    assign mem_byte = cpu_byte;
    assign cpu_rdata = mem_rdata;
    assign fault = 1'b0;
endmodule