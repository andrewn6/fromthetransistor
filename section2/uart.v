module uart (
    input wire clk, 
    input wire reset, 
    input wire rx,
    output reg tx,
    input wire [7:0] data_in,
    input wire wr_en,
    output reg [7:0] data_out,
    output reg rx_ready,
    output reg tx_ready
);
  

    parameter CLK_FREQ = 50000000; // 50 Mhz
    parameter BAUD_RATE = 115200; // (115200 bps)
    localparam BAUD_DIV = CLK_FREQ / BAUD_RATE;

    typedef enum logic [2:0] {
        IDLE,
        START,
        DATA,
        STOP
    } state_t;

    state_t rx_state, tx_state;
    reg [15:0] tx_baud_counter;
    reg [15:0] rx_baud_counter;
    reg [2:0] rx_bit_counter;
    reg [2:0] tx_bit_counter;
    reg [7:0] rx_shift_reg, tx_shift_reg;


    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
          rx_state <= IDLE;
          rx_bit_counter <= 0;
          rx_ready <= 0;
          rx_baud_counter <= 0;
          rx_shift_reg <= 0;
          data_out <= 0;

        end else begin 
            rx_ready <= 0;
            if (rx_state == IDLE) begin 
                if (~rx) begin rx_state <= START; rx_baud_counter <= 16'(BAUD_DIV / 2); end 
            end else if (rx_baud_counter == 16'(BAUD_DIV - 1)) begin 
                rx_baud_counter <= 0;
                case (rx_state)
                    START: if (~rx) begin rx_state <= DATA; rx_bit_counter <= 0; end else rx_state <= IDLE;
                    DATA: begin 
                        rx_shift_reg[rx_bit_counter] <= rx;
                        if (rx_bit_counter == 3'd7) rx_state <= STOP;
                        else rx_bit_counter <= rx_bit_counter + 1;
                    end 
                    STOP: begin rx_state <= IDLE; if (rx) begin data_out <= rx_shift_reg; rx_ready <= 1; end end 
                    default: rx_state <= IDLE;
                endcase 
            end else begin 
                rx_baud_counter <= rx_baud_counter + 1;
            end
        end
    end

    always @(posedge clk or posedge reset) begin 
        if (reset) begin 
            tx_state <= IDLE;
            tx_ready <= 1; 
            tx_bit_counter <= 0;
            tx_shift_reg <= 0;
            tx <= 1'b1;
            tx_baud_counter <= 0;
        end else begin 
            if (tx_state == IDLE) begin 
                if (wr_en) begin tx_state <= START; tx_shift_reg <= data_in; tx_ready <= 0; tx <= 1'b0; tx_baud_counter <= 0;
                end 
            end else if (tx_baud_counter == 16'(BAUD_DIV - 1)) begin 
                tx_baud_counter <= 0;
                case (tx_state)
                    START: begin tx_state <= DATA; tx_bit_counter <= 0; tx <= tx_shift_reg[0]; end 
                    DATA: begin 
                        if (tx_bit_counter == 3'd7) begin 
                            tx_state <= STOP;
                            tx <= 1'b1;
                        end else begin 
                            tx_bit_counter <= tx_bit_counter + 1;
                            tx <= tx_shift_reg[tx_bit_counter + 1];
                        end 
                    end 
                    STOP: begin tx_state <= IDLE; tx_ready <= 1; end 
                    default: tx_state <= IDLE;
                endcase 
            end else begin 
                tx_baud_counter <= tx_baud_counter + 1;
            end
        end
    end
endmodule
