`timescale 1ns / 1ps

// smoke test for the board: count the 50 mhz clock and show the top
// bits on the 8 leds, so led0 toggles about 3 times a second.
// key0 (active low) freezes the count, to prove the inputs work too.
module blink (
    input wire clk50,
    input wire key0_n,
    output wire [7:0] led
);
    reg [31:0] count = 32'd0;

    always @(posedge clk50)
        if (key0_n)
            count <= count + 32'd1;

    assign led = count[30:23];
endmodule
