// uart_tb.cpp - real tests for the fixed uart
//
// 1. tx framing: capture tx at mid-bit with the tb's own baud counter,
//    verify 8n1 (start 0, 8 data bits lsb first, stop 1)
// 2. rx direct: the tb drives rx on its own timing, byte must land in
//    data_out with an rx_ready pulse (proves the mid-bit resync)
// 3. loopback back-to-back: rx wired to tx, two bytes with only one
//    frame gap (proves the stop bit actually stops)
//
// build:
//   verilator --cc uart.v --exe uart_tb.cpp --top-module uart --trace \
//     -Mdir obj_dir_uart -o uart_sim
//   make -C obj_dir_uart -f Vuart.mk
#include <verilated.h>
#include <verilated_vcd_c.h>
#include "Vuart.h"
#include <cstdio>
#include <cstdint>

static Vuart* top;
static VerilatedVcdC* tfp;
static vluint64_t sim_time = 0;
static int failures = 0;

static const int BAUD_DIV = 434;  // 50mhz / 115200

static void tick() {
  top->clk = 0;
  top->eval();
  top->clk = 1;
  top->eval();
  sim_time += 20;  // one full 20ns clock period per tick
  if (tfp) tfp->dump(sim_time);
}

static void check(bool ok, const char* what) {
  printf("  %s: %s\n", ok ? "PASS" : "FAIL", what);
  if (!ok) failures++;
}

static void cycles(int n) {
  for (int i = 0; i < n; i++) tick();
}

// ---- tx side: send one byte, capture the wire, verify the frame ----

static void tx_send(uint8_t b) {
  top->data_in = b;
  top->wr_en = 1;
  tick();  // this posedge latches START and drives tx low
  top->wr_en = 0;
}

static bool tx_capture(uint8_t b) {
  int bits[10];
  for (int k = 0; k < 10; k++) {
    cycles(BAUD_DIV / 2);
    bits[k] = top->tx;  // mid-bit sample
    cycles(BAUD_DIV - BAUD_DIV / 2);
  }
  if (bits[0] != 0) return false;      // start bit
  if (bits[9] != 1) return false;      // stop bit
  for (int i = 0; i < 8; i++)
    if (bits[1 + i] != ((b >> i) & 1)) return false;
  return top->tx_ready == 1;           // back to idle
}

// ---- rx side: tb drives the line on its own clock ----

static bool rx_send(uint8_t b) {
  bool ready_seen = false;
  top->rx = 0;                          // start bit
  cycles(BAUD_DIV);
  for (int i = 0; i < 8; i++) {
    top->rx = (b >> i) & 1;
    cycles(BAUD_DIV);
  }
  top->rx = 1;                          // stop bit
  for (int c = 0; c < BAUD_DIV + 20; c++) {
    tick();
    if (top->rx_ready) ready_seen = true;
  }
  return ready_seen && top->data_out == b;
}

// ---- loopback: rx tied to tx, back-to-back bytes ----

static bool loopback(uint8_t b, bool second) {
  // wait until the transmitter is idle, then fire immediately: the
  // previous stop bit is the only gap the receiver gets
  for (int guard = 0; guard < 20 * BAUD_DIV && !top->tx_ready; guard++)
    tick();
  tx_send(b);
  bool ready_seen = false;
  for (int guard = 0; guard < 20 * BAUD_DIV; guard++) {
    top->rx = top->tx;  // the wire
    tick();
    if (top->rx_ready) ready_seen = true;
  }
  top->rx = 1;
  return ready_seen && top->data_out == b && !second ? true
       : ready_seen && top->data_out == b;
}

int main(int argc, char** argv) {
  Verilated::commandArgs(argc, argv);
  top = new Vuart;
  tfp = new VerilatedVcdC;
  Verilated::traceEverOn(true);
  top->trace(tfp, 99);
  tfp->open("uart.vcd");

  top->clk = 0;
  top->reset = 1;
  top->rx = 1;
  top->data_in = 0;
  top->wr_en = 0;
  cycles(5);
  top->reset = 0;
  cycles(5);

  printf("tx framing (capture at mid-bit):\n");
  tx_send(0x55);
  check(tx_capture(0x55), "tx 0x55 frame");
  tx_send(0xA3);
  check(tx_capture(0xA3), "tx 0xA3 frame");
  tx_send(0x00);
  check(tx_capture(0x00), "tx 0x00 frame");

  printf("rx direct (tb-driven timing, mid-bit resync):\n");
  check(rx_send(0xC3), "rx 0xC3");
  check(rx_send(0xFF), "rx 0xFF");
  check(rx_send(0x00), "rx 0x00");

  printf("loopback back-to-back:\n");
  check(loopback(0x11, false), "loopback 0x11");
  check(loopback(0x22, true), "loopback 0x22 right behind it");

  printf("\n%s (%d failures)\n",
         failures ? "SOME TESTS FAILED" : "ALL TESTS PASSED", failures);

  tfp->close();
  top->final();
  delete top;
  delete tfp;
  return failures ? 1 : 0;
}
