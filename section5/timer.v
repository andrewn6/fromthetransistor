`timescale 1ns / 1ps 

module timer (
    input wire clk,
    input wire reset,
    input wire [3:0] addr,
    input wire [31:0] wdata,
    input wire we,
    output wire [31:0] rdata,
    output wire irq 
);
    reg [31:0] load;
    reg [31:0] count;
    reg pending;

    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
            load <= 32'd0;
            count <= 32'd0;
            pending <= 1'b0;
        end else begin 
            if (we && addr == 4'h0) begin 
                load <= wdata;
                count <= wdata;
            end else if (load != 32'd0) begin 
                if (count <= 32'd1) begin 
                    count <= load;
                    pending <= 1'b1;
                end else begin 
                    count <= count - 32'd1;
                end 
            end 
            if (we && addr == 4'h8 && wdata[0])
            pending <= 1'b0;
        end 
    end 

    assign rdata = (addr == 4'h0) ? load :
                   (addr == 4'h4) ? count : 
                   (addr == 4'h8) ? {31'd0, pending} : 32'd0;
    assign irq = pending;
endmodule