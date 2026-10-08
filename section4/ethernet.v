`timescale 1ns / 1ps


module ethernet (
  input wire clk,
  input wire reset,

  // MMIO bus (same shape as CPU SRAM interface)
  input wire [31:0] addr,
  input wire [31:0] data_in,
  input wire        wr_en,
  input wire        rd_en,
  output reg [31:0] data_out,

  // PHY side: the physical ethernet chip, 4 bits at a time
  input wire        mii_tx_clk,
  input wire        mii_rx_clk,
  input wire  [3:0] mii_rxd,
  input wire        mii_rx_dv,
  output reg        mii_tx_en,
  output reg [3:0]  mii_txd
);

  // addresses the CPU uses to reach each register
  localparam CTRL_ADDR = 32'h0000; // bit0 = start tx
  localparam STATUS_ADDR = 32'h0004; // bit0 = tx_busy, bit1 = rx_ready
  localparam TXLEN_ADDR = 32'h0008; // bytes to send
  localparam RXLEN_ADDR = 32'h000C; // bytes received
  localparam TXBUF_BASE = 32'h1000; // fram bytes to send
  localparam RXBUF_BASE = 32'h2000; // received frame bytes

  localparam BUF_SIZE = 1536;

  // internal memory: things the chip remembers
  reg [10:0] tx_len;
  reg [10:0] rx_len;
  reg        tx_start;
  reg        tx_busy;
  reg        rx_ready;

  // frame buffers, the memoryes inside the chip
  reg [7:0] tx_buf [0:BUF_SIZE-1];
  reg [7:0] rx_buf [0:BUF_SIZE-1];

  // This is our write path, every clock tick, if a CPU is writing, store what
  // it sent
  always @(posedge clk or posedge reset) begin
    if (reset) begin
      tx_len <= 0;
      tx_start <= 0;

    end else begin
      tx_start <= 0;
      if (wr_en) begin
          case (addr)
            CTRL_ADDR: tx_start <= data_in[0];
            TXLEN_ADDR: tx_len <= data_in[10:0];
            default: begin
              if (addr >= TXBUF_BASE && addr < TXBUF_BASE + BUF_SIZE)
                  tx_buf[addr - TXBUF_BASE] <= data_in[7:0];
            end
          endcase
      end
    end
  end
  // Read path, when CPU reads we hand back the right value
  always @(*) begin
      data_out = 32'h0;
      if (rd_en) begin
          case(addr)
              STATUS_ADDR: data_out = {30'b0, rx_ready, tx_busy};
              RXLEN_ADDR:  data_out = {21'b0, rx_len};
              default: begin
                if (addr >= RXBUF_BASE && addr < RXBUF_BASE + BUF_SIZE)
                    data_out = {24'b0, rx_buf[addr - RXBUF_BASE]};
              end
          endcase
      end
  end

  typedef enum logic [2:0] {
    TX_IDLE, // wait for the CPU to say go!
    TX_PREAMBLE, // send 7 bytes so PHY can sync
    TX_SFD, // send 0x5, the frame starts now
    TX_DATA, // send the actual frame bytes from tx_buf
    TX_IPG // quiet gap required between frames
  } tx_state_t;

  tx_state_t tx_state;
  reg [10:0] tx_byte_idx;
  reg        tx_nibble;
  reg [3:0]  tx_preamble_count;
  reg [3:0]  tx_ipg_count;

  always @(posedge clk or posedge reset) begin
      if (reset) begin
          tx_state <= TX_IDLE;
          mii_tx_en <= 0;
          mii_txd   <= 4'h0;
          tx_busy   <= 0;
          tx_byte_idx <= 0;
          tx_nibble   <= 0;
          tx_preamble_count <= 0;
          tx_ipg_count <= 0;
      end else begin
          case (tx_state)
              TX_IDLE: begin
                mii_tx_en <= 0;
                tx_busy <= 0;
                if (tx_start) begin
                  tx_busy <= 1;
                  tx_state <= TX_PREAMBLE;
                  tx_preamble_count <= 0;
                  tx_nibble <= 0;
                end
              end

            // send 7 copies of 0x55 - each byte is two 0x5 nibbles
            TX_PREAMBLE: begin
              mii_tx_en <= 1;
              mii_txd <= 4'h5;
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                if (tx_preamble_count == 4'd6)
                    tx_state <= TX_SFD;
                else
                  tx_preamble_count <= tx_preamble_count + 1;
              end
            end

            // send 0xD5: low nibble 0x5 first, then high nibble 0Xd
            TX_SFD: begin
              mii_txd <= tx_nibble ? 4'hD : 4'h5;
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                tx_state <= TX_DATA;
                tx_byte_idx <= 0;
              end
            end

            // send each byte of tx_buf, low nibble first
            TX_DATA: begin
              mii_tx_en <= 1;
              mii_txd <= tx_nibble ? tx_buf[tx_byte_idx][7:4] : tx_buf[tx_byte_idx][3:0];
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                if (tx_byte_idx == tx_len - 1) begin
                  tx_state <= TX_IPG;
                  tx_ipg_count <= 0;
                end else
                  tx_byte_idx <= tx_byte_idx + 1;
              end
            end

            TX_IPG: begin
              mii_tx_en <= 0;
              mii_txd   <= 4'h0;
              if (tx_ipg_count == 4'd11) // 12 nibble gap
                tx_state <= TX_IDLE;
              else
                tx_ipg_count <= tx_ipg_count + 1;
            end

            default: tx_state <= TX_IDLE;
          endcase
      end
end

// RX
// This watches the PHY, when a frame arrives it skip the premable/SFD

typedef enum logic [1:0] {
  RX_IDLE,
  RX_PREAMBLE,
  RX_DATA
} rx_state_t;

rx_state_t rx_state;
reg [10:0] rx_byte_idx;
reg        rx_nibble;
reg [3:0]  rx_low;

always @(posedge clk or posedge reset) begin
  if (reset) begin 
  
      rx_state    <= RX_IDLE;
      rx_ready    <= 0;
      rx_len      <= 0;
      rx_byte_idx <= 0;
      rx_nibble   <= 0;

  end else begin 
    case (rx_state)
      RX_IDLE: begin 
        rx_ready <= 0; // clear old frame ready flag 
        if (mii_rx_dv) begin 
            rx_state <= RX_PREAMBLE;
        end 
      end

      RX_PREAMBLE: begin 
        if (!mii_rx_dv) begin 
          rx_state <= RX_IDLE;
        end else if (mii_rxd == 4'hD) begin 
          rx_state <= RX_DATA;
          rx_byte_idx <= 0;
          rx_nibble <= 0; 
        end
      end 

      RX_DATA: begin 
        if (!mii_rx_dv) begin 
          rx_len <= rx_byte_idx;
          rx_ready <= 1;
          rx_state <= RX_IDLE;
        end else begin 
          if (!rx_nibble) begin 
            rx_low <= mii_rxd;
            rx_nibble <= 1;
          end else begin 
            rx_buf[rx_byte_idx] <= {mii_rxd, rx_low};
            rx_byte_idx <= rx_byte_idx + 1;
            rx_nibble <= 0;
          end 
        end 
      end 
        default: rx_State <= RX_IDLE;
    endcase
  end
end
endmodule
