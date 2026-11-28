// UART tiny address decoder 

// memory mapings:
// 0x40001000 uart DATA 
// 0x40001004 uart STATUS
// everything else 
`timescale 1ns / 1ps 

module soc (
    input wire clk,
    input wire reset,
    input wire [31:0] mem_rdata,
    output wire [31:0] mem_addr,
    output wire [31:0] mem_wdata,
    output wire mem_we,
    output wire mem_byte,
    // serial 
    input wire uart_rx,
    output wire uart_tx,
    output wire halted,

    input wire mii_tx_clk,
    input wire mii_rx_clk,
    input wire[3:0] mii_rxd,
    input wire mii_rx_dv,
    output wire [3:0] mii_txd,
    output wire mii_tx_en,
    output wire mdc,
    inout wire mdio
);

    wire [31:0] va;
    wire [31:0] va_wdata;
    wire [31:0] va_rdata;
    wire va_we;
    wire va_byte;
    wire mem_req;
    wire stall;
    wire fault;
    wire cpu_user;
    wire timer_irq;

    wire [31:0] cpu_addr;
    wire [31:0] cpu_wdata;
    wire cpu_we;
    wire cpu_byte;
    reg [31:0] cpu_rdata;

    arm7_cpu cpu (
        .clk(clk),
        .reset(reset),
        .sram_data_in(va_rdata),
        .sram_addr(va),
        .sram_data_out(va_wdata),
        .sram_we(va_we),
        .sram_byte(va_byte),
        .halted(halted),
        .stall(stall),
        .mem_req(mem_req),
        .abort(fault),
        .user_mode(cpu_user),
        .irq(timer_irq)
    );

    mmu mmu0(
        .clk(clk),
        .reset(reset),
        .va(va),
        .cpu_wdata(va_wdata),
        .cpu_we(va_we),
        .cpu_byte(va_byte),
        .mem_req(mem_req),
        .user(cpu_user),
        .cpu_rdata(va_rdata),
        .stall(stall),
        .fault(fault),
        .pa(cpu_addr),
        .mem_wdata(cpu_wdata),
        .mem_we(cpu_we),
        .mem_byte(cpu_byte),
        .mem_rdata(cpu_rdata)
    );

    // 16-byte uart window at 0x40001000
    wire uart_sel = (cpu_addr[31:4] == 28'h4000100);
    wire [3:0] uart_reg = cpu_addr[3:0];

    wire [7:0] rx_byte;
    wire rx_pulse;
    wire tx_ready;
    reg rx_valid;

    uart u (
        .clk(clk),
        .reset(reset),
        .rx(uart_rx),
        .tx(uart_tx),
        .data_in(cpu_wdata[7:0]),
        .wr_en(uart_sel && cpu_we && uart_reg == 4'h0),
        .data_out(rx_byte),
        .rx_ready(rx_pulse),
        .tx_ready(tx_ready)
    );

    wire data_read = uart_sel && !cpu_we && uart_reg == 4'h0;
    always @(posedge clk or posedge reset) begin 
        if (reset) rx_valid <= 1'b0;
        else if (rx_pulse) rx_valid <= 1'b1;
        else if (data_read) rx_valid <= 1'b0;
    end 

    localparam ETH_BASE = 32'h40002000;
    wire eth_sel = (cpu_addr >= ETH_BASE) && (cpu_addr < 32'h40005000);
    wire [31:0] eth_addr = cpu_addr - ETH_BASE;
    wire [31:0] eth_rdata;

    ethernet eth (
        .clk(clk),
        .reset(reset),
        .addr(eth_addr),
        .data_in(cpu_wdata),
        .wr_en(eth_sel && cpu_we),
        .rd_en(eth_sel && !cpu_we),
        .data_out(eth_rdata),
        .mii_tx_clk(mii_tx_clk),
        .mii_rx_clk(mii_rx_clk),
        .mii_rxd(mii_rxd),
        .mii_rx_dv(mii_rx_dv),
        .mii_tx_en(mii_tx_en),
        .mii_txd(mii_txd),
        .mdc(mdc),
        .mdio(mdio)
    );

    wire timer_sel = (cpu_addr[31:4] == 28'h4000700);
    wire [31:0] timer_rdata;

    timer t (
        .clk(clk),
        .reset(reset),
        .addr(cpu_addr[3:0]),
        .wdata(cpu_wdata),
        .we(timer_sel && cpu_we),
        .rdata(timer_rdata),
        .irq(timer_irq)
    );

    always @(*) begin 
        if (uart_sel) begin 
            case (uart_reg)
            4'h0: cpu_rdata = {24'b0, rx_byte};
            4'h4: cpu_rdata = {30'b0, rx_valid, tx_ready};
            default: cpu_rdata = 32'd0;
            endcase 
        end else if (eth_sel) begin 
            cpu_rdata = eth_rdata;
        end else if (timer_sel) begin 
            cpu_rdata = timer_rdata;
        end else begin
            cpu_rdata = mem_rdata;
        end
    end 


    assign mem_addr = cpu_addr;
    assign mem_wdata = cpu_wdata;
    assign mem_we = cpu_we && !uart_sel && !eth_sel && !timer_sel;
    assign mem_byte = cpu_byte;
endmodule
