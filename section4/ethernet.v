`timescale 1ns / 1ps


module ethernet (
  input wire clk,         // cpu bus clock
  input wire reset,

  // MMIO bus (same shape as CPU SRAM interface)
  input wire [31:0] addr,
  input wire [31:0] data_in,
  input wire        wr_en,
  input wire        rd_en,
  output reg [31:0] data_out,

  // PHY side: the physical ethernet chip, 4 bits at a time
  input wire        mii_tx_clk,  // tx fsm runs on this clock
  input wire        mii_rx_clk,  // rx fsm runs on this clock
  input wire  [3:0] mii_rxd,
  input wire        mii_rx_dv,
  output reg        mii_tx_en,
  output reg  [3:0] mii_txd,

  // PHY management side (MDIO config channel)
  output reg        mdc,  // mdio clock we generate
  inout  wire       mdio  // bidirectional data line
);

  // addresses the CPU uses to reach each register
  localparam CTRL_ADDR = 32'h0000;      // bit0 = start tx, bit1 = ack rx
  localparam STATUS_ADDR = 32'h0004;    // bit0 tx_busy, bit1 rx_ready, bit2 mdio_busy, bit3 rx_crc_ok
  localparam TXLEN_ADDR = 32'h0008;     // payload bytes to send, fcs appended automatically
  localparam RXLEN_ADDR = 32'h000C;     // payload + fcs bytes received
  localparam MDIO_CTRL_ADDR = 32'h0010; // write a 32-bit mdio frame + go
  localparam MDIO_DATA_ADDR = 32'h0014; // read: 16-bit value from the phy
  localparam TXBUF_BASE = 32'h1000;     // payload bytes to send
  localparam RXBUF_BASE = 32'h2000;    // received frame bytes incl fcs

  localparam BUF_SIZE = 1536;

  // 802.3 fcs crc. bytes are processed lsb-first, exactly the order
  // bits leave on the wire (low nibble first, bit 0 first in a nibble).
  // tx appends the crc value little-endian after the payload; a
  // receiver that includes those 4 bytes lands on the residue
  function [31:0] crc_step(input [31:0] c_in, input [7:0] b);
    integer i;
    reg [31:0] c;
    begin
      c = c_in;
      for (i = 0; i < 8; i = i + 1)
        c = ((c[0] ^ b[i])) ? ({1'b0, c[31:1]} ^ 32'hEDB88320) : {1'b0, c[31:1]};
      crc_step = c;
    end
  endfunction

  // internal memory: things the chip remembers
  reg [10:0] tx_len;
  reg [10:0] rx_len;      // written by the rx fsm, read after rx_ready

  // frame buffers. tx_buf is filled by the cpu and read by the tx fsm,
  // rx_buf the other way around. crossing them raw is fine: the data
  // is stable before the request crosses domains and stays stable
  // until the response comes back
  reg [7:0] tx_buf [0:BUF_SIZE-1];
  reg [7:0] rx_buf [0:BUF_SIZE-1];

  // ---------- clock domain crossings ----------
  // cpu -> tx: a toggle in the clk domain, edge-detected in the
  // mii_tx_clk domain. a raw one-cycle pulse can fall between two mii
  // edges and get lost. (the cpu must poll tx_busy before issuing the
  // next frame, or two toggles can alias before the fsm sees the first)
  reg        tx_go_t;
  reg [1:0] tx_go_sync;
  reg        tx_go_prev;

  always @(posedge mii_tx_clk or posedge reset) begin
    if (reset) begin
      tx_go_sync <= 2'b00;
      tx_go_prev <= 1'b0;
    end else begin
      tx_go_sync <= {tx_go_sync[0], tx_go_t};
      tx_go_prev <= tx_go_sync[1];
    end
  end
  wire tx_start = (tx_go_sync[1] != tx_go_prev);

  // tx -> cpu: tx_busy is a slow level flag, double-flop it across.
  // the fsm clears it in the mii domain, the cpu polls the synced copy
  reg        tx_busy;
  reg [1:0] tx_busy_sync;

  // rx -> cpu: frame-done toggle in the rx domain, edge-detected in the
  // clk domain into a level flag that holds until the cpu acks it.
  // the old design cleared rx_ready by itself one cycle after the
  // frame landed, so a polling cpu never saw it
  reg        rx_done_t;
  reg [1:0] rx_done_sync;
  reg        rx_done_prev;
  reg        rx_ready;
  reg        rx_ack;     // one-shot: cpu read the frame out
  reg        rx_crc_ok;  // rx domain: did the last frame pass its crc
  reg [1:0] rx_crc_sync;

  always @(posedge clk or posedge reset) begin
    if (reset) begin
      tx_busy_sync <= 2'b00;
      rx_done_sync <= 2'b00;
      rx_done_prev <= 1'b0;
      rx_ready     <= 1'b0;
      rx_crc_sync  <= 2'b00;
    end else begin
      tx_busy_sync <= {tx_busy_sync[0], tx_busy};
      rx_done_sync <= {rx_done_sync[0], rx_done_t};
      rx_done_prev <= rx_done_sync[1];
      rx_crc_sync  <= {rx_crc_sync[0], rx_crc_ok};

      if (rx_done_sync[1] != rx_done_prev)
        rx_ready <= 1'b1;   // frame landed, hold until the cpu acks
      else if (rx_ack)
        rx_ready <= 1'b0;   // cpu read it out, drop the flag
    end
  end

  // ---------- cpu write path (clk domain) ----------
  reg        mdio_go;
  reg [31:0] mdio_frame;

  // this is our write path, every clock tick, if a cpu is writing,
  // store what it sent
  always @(posedge clk or posedge reset) begin
    if (reset) begin
      tx_len <= 0;
      tx_go_t <= 0;
      rx_ack <= 0;
      mdio_go <= 0;
    end else begin
      rx_ack <= 0;              // one-shot, clears every tick
      mdio_go <= 0;             // one-shot, clears every tick
      if (wr_en) begin
          case (addr)
            CTRL_ADDR: begin
              if (data_in[0]) tx_go_t <= ~tx_go_t;
              rx_ack <= data_in[1];        // cpu acks the rx frame
            end
            TXLEN_ADDR: tx_len <= (data_in[10:0] > 11'd1532) ? 11'd1532 : data_in[10:0];
            MDIO_CTRL_ADDR: begin  // cpu loads a frame and says go
              mdio_frame <= data_in;
              mdio_go    <= 1;
            end
            default: begin
              if (addr >= TXBUF_BASE && addr < TXBUF_BASE + BUF_SIZE)
                  tx_buf[addr - TXBUF_BASE] <= data_in[7:0];
            end
          endcase
      end
    end
  end

  // ---------- read path ----------
  // read mux, combinational
  always_comb begin
      data_out = 32'h0;
      if (rd_en) begin
          case(addr)
              STATUS_ADDR: data_out = {28'b0, rx_crc_sync[1], mdio_busy, rx_ready, tx_busy_sync[1]};
              RXLEN_ADDR:  data_out = {21'b0, rx_len};
              MDIO_DATA_ADDR: data_out = {16'b0, mdio_read_data};
              default: begin
                if (addr >= RXBUF_BASE && addr < RXBUF_BASE + BUF_SIZE)
                    data_out = {24'b0, rx_buf[addr - RXBUF_BASE]};
              end
          endcase
      end
  end

  // ---------- tx fsm (mii_tx_clk domain) ----------
  typedef enum logic [2:0] {
    TX_IDLE,      // wait for the cpu to say go
    TX_PREAMBLE,  // send 7 bytes of 0x55 so the phy can sync
    TX_SFD,       // send 0xD5, the frame starts now
    TX_DATA,      // send the payload bytes from tx_buf
    TX_FCS,       // send the 4 crc bytes we computed
    TX_IPG        // 96 bit-times of quiet between frames
  } tx_state_t;

  tx_state_t tx_state;
  reg [10:0] tx_byte_idx;
  reg        tx_nibble;
  reg [2:0]  tx_preamble_count;
  reg [4:0]  tx_ipg_count;
  reg [31:0] tx_crc;
  reg [31:0] fcs_word;
  reg [1:0]  tx_fcs_idx;

  always @(posedge mii_tx_clk or posedge reset) begin
      if (reset) begin
          tx_state <= TX_IDLE;
          mii_tx_en <= 0;
          mii_txd   <= 4'h0;
          tx_busy   <= 0;
          tx_byte_idx <= 0;
          tx_nibble   <= 0;
          tx_preamble_count <= 0;
          tx_ipg_count <= 0;
          tx_crc <= 32'hFFFFFFFF;
          fcs_word <= 0;
          tx_fcs_idx <= 0;
      end else begin
          case (tx_state)
              TX_IDLE: begin
                mii_tx_en <= 0;
                tx_busy   <= 0;
                if (tx_start && tx_len != 11'd0) begin
                  tx_busy <= 1;
                  tx_crc  <= 32'hFFFFFFFF;
                  tx_preamble_count <= 0;
                  tx_nibble <= 0;
                  tx_state <= TX_PREAMBLE;
                end
              end

            // send 7 copies of 0x55 - each byte is two 0x5 nibbles
            TX_PREAMBLE: begin
              mii_tx_en <= 1;
              mii_txd <= 4'h5;
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                if (tx_preamble_count == 3'd6)
                    tx_state <= TX_SFD;
                else
                  tx_preamble_count <= tx_preamble_count + 1;
              end
            end

            // send 0xD5: low nibble 0x5 first, then high nibble 0xD
            TX_SFD: begin
              mii_txd <= tx_nibble ? 4'hD : 4'h5;
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                  tx_state <= TX_DATA;
                  tx_byte_idx <= 0;
              end
            end

            // send each payload byte, low nibble first, feeding the crc
            TX_DATA: begin
              mii_tx_en <= 1;
              mii_txd <= tx_nibble ? tx_buf[tx_byte_idx][7:4] : tx_buf[tx_byte_idx][3:0];
              if (!tx_nibble)
                  tx_crc <= crc_step(tx_crc, tx_buf[tx_byte_idx]);
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                  if (tx_byte_idx == tx_len - 1) begin
                    fcs_word   <= ~tx_crc;
                    tx_fcs_idx <= 0;
                    tx_state   <= TX_FCS;
                  end else
                    tx_byte_idx <= tx_byte_idx + 1;
              end
            end

            // send the 4 crc bytes, little-endian, low nibble first
            TX_FCS: begin
              mii_tx_en <= 1;
              mii_txd <= tx_nibble ? fcs_word[4+8*tx_fcs_idx +: 4] : fcs_word[8*tx_fcs_idx +: 4];
              tx_nibble <= ~tx_nibble;
              if (tx_nibble) begin
                  if (tx_fcs_idx == 2'd3)
                    tx_state <= TX_IPG;
                  else
                    tx_fcs_idx <= tx_fcs_idx + 1;
              end
            end

            TX_IPG: begin
              mii_tx_en <= 0;
              mii_txd   <= 4'h0;
              if (tx_ipg_count == 5'd23) // 96 bit times = 24 nibbles
                tx_state <= TX_IDLE;
              else
                tx_ipg_count <= tx_ipg_count + 1;
            end

            default: tx_state <= TX_IDLE;
          endcase
      end
  end

  // ---------- rx fsm (mii_rx_clk domain) ----------
  // watches the phy, skips preamble/sfd, reassembles nibbles into bytes
  // and crc-checks the whole frame including the trailing fcs

  typedef enum logic [1:0] {
    RX_IDLE,
    RX_PREAMBLE,
    RX_DATA
  } rx_state_t;

  rx_state_t rx_state;
  reg [10:0] rx_byte_idx;
  reg        rx_nibble;
  reg [3:0]  rx_low;
  reg [31:0] rx_crc;

  always @(posedge mii_rx_clk or posedge reset) begin
    if (reset) begin
        rx_state    <= RX_IDLE;
        rx_len      <= 0;
        rx_byte_idx <= 0;
        rx_nibble   <= 0;
        rx_low      <= 0;
        rx_crc      <= 32'hFFFFFFFF;
        rx_crc_ok   <= 0;
        rx_done_t   <= 1'b0;
    end else begin
        case (rx_state)
          RX_IDLE: begin
            if (mii_rx_dv) begin
                rx_state <= RX_PREAMBLE;
            end
          end

          RX_PREAMBLE: begin
            if (!mii_rx_dv) begin
              rx_state <= RX_IDLE;
            end else if (mii_rxd == 4'hD) begin
              // sfd done (0x5 then 0xD), payload nibbles start now
              rx_crc      <= 32'hFFFFFFFF;
              rx_byte_idx <= 0;
              rx_nibble   <= 0;
              rx_state    <= RX_DATA;
            end
          end

          RX_DATA: begin
            if (!mii_rx_dv) begin
              if (rx_nibble == 1'b0) begin
                // clean byte-aligned end: publish the frame
                rx_len    <= rx_byte_idx;
                rx_crc_ok <= (rx_crc == 32'hDEBB20E3);
                rx_done_t <= ~rx_done_t;   // frame-complete toggle -> cpu domain
              end
              // else: frame died mid-byte, drop it entirely
              rx_state <= RX_IDLE;
            end else if (rx_byte_idx >= BUF_SIZE) begin
              // oversized, drop it
              rx_state <= RX_IDLE;
            end else begin
              if (!rx_nibble) begin
                  rx_low <= mii_rxd;
                  rx_nibble <= 1;
              end else begin
                  rx_buf[rx_byte_idx] <= {mii_rxd, rx_low};
                  rx_crc <= crc_step(rx_crc, {mii_rxd, rx_low});
                  rx_byte_idx <= rx_byte_idx + 1;
                  rx_nibble   <= 0;
              end
            end
          end
          default: rx_state <= RX_IDLE;
        endcase
    end
  end

  // ---------- mdio management (clk domain) ----------
  // frame layout: ST(2) OP(2) PHYAD(5) REG(5) TA(2) DATA(16), msb of
  // mdio_frame goes out first. bits 0..13 are ours to drive, 14/15 are
  // turnaround, 16..31 are the phy reply (reads only). we present each
  // bit at the falling mdc edge so the phy samples it at the rise, and
  // we sample the phy's reply at the rise, where it is stable
  reg [15:0] mdio_read_data; // value shifted back from the phy
  reg        mdio_busy;       // doing a transfer
  reg        mdio_is_read;    // latched from the op field at go
  reg [5:0]  mdio_bit;        // which of the 32 bits we are on
  reg        mdio_out;        // what we drive onto the mdio line
  reg        mdio_drive;      // 1 = we drive the line, 0 = we listen
  reg [8:0]  mdc_div;         // slows mdc down (mdio is a slow bus)

  assign mdio = mdio_drive ? mdio_out : 1'bz;

  always @(posedge clk or posedge reset) begin
    if (reset) begin
        mdc <= 0;
        mdio_busy <= 0;
        mdio_is_read <= 0;
        mdio_bit <= 0;
        mdio_out <= 0;
        mdio_drive <= 0;
        mdc_div <= 0;
        mdio_read_data <= 0;
    end else if (!mdio_busy) begin
        if (mdio_go) begin
            mdio_busy    <= 1;
            mdio_bit     <= 0;
            mdio_drive   <= 1;
            mdc_div      <= 0;
            mdc          <= 1'b0;   // park the clock low, first toggle rises
            mdio_is_read <= (mdio_frame[29:28] == 2'b00);
            mdio_out     <= mdio_frame[31];  // bit 0: phy samples it at the first rise
            mdio_frame   <= {mdio_frame[30:0], 1'b0};
        end
    end else begin
        // slow the clock way down: only step on divider rollover
        if (mdc_div == 9'd499) begin
          mdc_div <= 0;
          mdc     <= ~mdc;
          if (!mdc) begin
            // rising edge: the phy just sampled our bit. if this is a
            // read, its reply bit went out at the last fall and is
            // stable here: sample it now
            if (mdio_is_read && mdio_bit >= 6'd16)
              mdio_read_data <= {mdio_read_data[14:0], mdio};
            if (mdio_bit == 6'd31) begin
              mdio_busy  <= 0;
              mdio_drive <= 0;
            end else
              mdio_bit <= mdio_bit + 1;
          end else begin
            // falling edge: bit period mdio_bit starts, present its
            // value so the phy can sample it at the next rise
            if (!mdio_is_read)
              mdio_out <= mdio_frame[31];        // write: we drive all 32 bits
            else if (mdio_bit <= 6'd13)
              mdio_out <= mdio_frame[31];        // read: st/op/phyad/regad
            else if (mdio_bit == 6'd14)
              mdio_drive <= 0;                   // turnaround: hand the line to the phy
            mdio_frame <= {mdio_frame[30:0], 1'b0};
          end
        end else
          mdc_div <= mdc_div + 1;
    end
  end

endmodule
