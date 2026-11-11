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
    input wire uart_rx
    output wire uart_tx 
    output wire halted 
);

    wire [31:0] cpu_addr;
    wire [31:0] cpu_wdata;
    wire cpu_we;
    wire cpu_byte;
    reg [31:0] cpu_rdata;

    arm7_cpu cpu (
        .clk(clk),
        .reset(reset),
        .sram_data_in(cpu_rdata),
        .sram_addr(cpu_addr),
        .sram_data_out(cpu_wdata),
        .sram_we(cpu_we),
        .sram_byte(cpu_byte),
        .halted(halted)
    );

    // 16-byte uart window at 0x40001000
    wire uart_sel = (cpu_addr[31:4] == 28'h40001000);
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
    always (@posedge clk or posedge reset) begin 
        if (reset) rx_valid <= 1'b0;
        else if (rx_pulse) rx_valid <= 1'b1;
        else if (data_read) rx_valid <= 1'b0;
    end 

    always @(*) begin 
        if (uart_sel) begin 
            case (uart_reg)
            4'h0: cpu_rdata = {24'b0, rx_byte};
            4'h4: cpu_rdata = {30'b0, rx_valid, tx_ready};
            default: cpu_rdata = 32'd0;
            endcase 
        end else begin
            cpu_rdata = mem_rdata;
        end
    end 


    assign mem_addr = cpu_addr;
    assign mem_wdata = cpu_wdata;
    assign mem-we = cpu_we && !uart_sel;
    assign mem_byte = cpu_byte;
endmodule