// ethernet_tb.cpp - verilator testbench for ethernet.v
//
// the tb plays both sides of the chip:
//   - the CPU: drives addr/data_in/wr_en on the MMIO bus
//   - the PHY: drives mii_rxd/mii_rx_dv, watches mii_txd/mii_tx_en
//
// three tests, in order:
//   1. tx:   send a frame, capture every nibble on the wire, verify
//            preamble + sfd + low-nibble-first payload
//   2. rx:   pretend to be a phy, feed a framed byte stream, verify
//            rx_ready/rx_len/rx_buf through MMIO
//   3. loopback: wire mii_txd -> mii_rxd and mii_tx_en -> mii_rx_dv,
//            send a frame, verify we receive the identical frame.
//            the one test that proves tx and rx agree on the wire format.
//
// build:
//   verilator --trace --cc ethernet.v --exe ethernet_tb.cpp
//   make -C obj_dir -f Vethernet.mk
//   ./obj_dir/Vethernet        # writes ethernet.vcd
#include <verilated.h>
#include "Vethernet.h"
#include <verilated_vcd_c.h>
#include <cstdio>
#include <vector>
#include <cstdint>

// mmio register map (must match ethernet.v localparams)
enum {
    CTRL_ADDR     = 0x0000,   // bit0 = start tx
    STATUS_ADDR   = 0x0004,   // bit0 tx_busy, bit1 rx_ready, bit2 mdio_busy
    TXLEN_ADDR    = 0x0008,
    RXLEN_ADDR    = 0x000C,
    MDIO_CTRL_ADDR= 0x0010,
    MDIO_DATA_ADDR= 0x0014,
    TXBUF_BASE    = 0x1000,
    RXBUF_BASE    = 0x2000,
};

static Vethernet* top;
static VerilatedVcdC* tfp;
static vluint64_t sim_time = 0;
static int failures = 0;

// eval with waveform dump
static void eval() {
    top->eval();
    if (tfp) tfp->dump(sim_time);
}

// one full clock cycle (rising edge in the middle)
static void tick() {
    top->clk = 0; eval(); sim_time++;
    top->clk = 1; eval(); sim_time++;
}

static void mmio_write(uint32_t a, uint32_t v) {
    top->addr = a; top->data_in = v;
    top->wr_en = 1; top->rd_en = 0;
    tick();                      // captured at this posedge
    top->wr_en = 0; top->addr = 0; top->data_in = 0;
}

// reads are combinational (always_comb in the dut), so we never need a
// clock edge to fetch a value. this matters: rx_ready is only held for
// one cycle after a frame lands, so we must read it edge-free.
static uint32_t mmio_read(uint32_t a) {
    top->addr = a; top->rd_en = 1; top->wr_en = 0;
    top->clk = 0; eval();        // settle, no posedge
    uint32_t v = top->data_out;
    top->rd_en = 0; top->addr = 0;
    return v;
}

static void check(bool ok, const char* what) {
    printf("  %s: %s\n", ok ? "PASS" : "FAIL", what);
    if (!ok) failures++;
}

// a small ethernet frame: dst mac, src mac, ipv4 ethertype, payload
static const int FRAME_LEN = 19;
static const uint8_t frame[FRAME_LEN] = {
    0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,     // dst
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66,     // src
    0x08, 0x00,                             // ethertype
    'h', 'e', 'l', 'l', 'o'                 // payload
};

// verify a captured nibble stream against the frame: 14 preamble nibbles,
// sfd (5 then D), then each byte low nibble first
static bool check_wire_order(const std::vector<uint8_t>& nibs) {
    if (nibs.size() != (size_t)(16 + 2 * FRAME_LEN)) {
        printf("    expected %d nibbles, captured %zu\n",
               16 + 2 * FRAME_LEN, nibs.size());
        return false;
    }
    for (int i = 0; i < 14; i++)
        if (nibs[i] != 0x5) { printf("    preamble nibble %d != 5\n", i); return false; }
    if (nibs[14] != 0x5) { printf("    sfd low nibble != 5\n"); return false; }
    if (nibs[15] != 0xD) { printf("    sfd high nibble != D\n"); return false; }
    for (int i = 0; i < FRAME_LEN; i++) {
        uint8_t lo = frame[i] & 0xF, hi = frame[i] >> 4;
        if (nibs[16 + 2*i] != lo || nibs[17 + 2*i] != hi) {
            printf("    byte %d nibbles wrong (got %X %X want %X %X)\n",
                   i, nibs[16 + 2*i], nibs[17 + 2*i], lo, hi);
            return false;
        }
    }
    return true;
}

// load the frame into the tx buffer and hit go
static void start_tx() {
    for (int i = 0; i < FRAME_LEN; i++)
        mmio_write(TXBUF_BASE + i, frame[i]);
    mmio_write(TXLEN_ADDR, FRAME_LEN);
    mmio_write(CTRL_ADDR, 1);
}

// ---------------- test 1: transmit ----------------
static void test_tx() {
    printf("test 1: tx (cpu -> phy)\n");
    start_tx();

    // capture every nibble while tx_en is high
    std::vector<uint8_t> nibs;
    for (int c = 0; c < 2000 && (int)nibs.size() < 16 + 2 * FRAME_LEN; c++) {
        tick();
        if (top->mii_tx_en)
            nibs.push_back(top->mii_txd);
    }
    // let the ipg finish so tx_busy clears
    for (int c = 0; c < 20; c++) tick();

    check(check_wire_order(nibs), "wire order: preamble, sfd, low-nibble-first data");
    check((mmio_read(STATUS_ADDR) & 1) == 0, "tx_busy cleared after ipg");
}

// ---------------- test 2: receive ----------------
static void test_rx() {
    printf("test 2: rx (phy -> cpu)\n");
    // pretend to be the phy: raise dv, feed preamble, sfd, frame, drop dv
    auto nib = [&](uint8_t n) {
        top->mii_rxd = n; top->mii_rx_dv = 1;
        tick();
    };
    for (int i = 0; i < 14; i++) nib(0x5);   // preamble
    nib(0x5); nib(0xD);                       // sfd
    for (int i = 0; i < FRAME_LEN; i++) {     // payload, low nibble first
        nib(frame[i] & 0xF);
        nib(frame[i] >> 4);
    }
    top->mii_rx_dv = 0;
    tick();                                   // rx finalizes on this edge

    // read immediately, edge-free: rx_ready pulses for a single cycle
    uint32_t status = mmio_read(STATUS_ADDR);
    check((status >> 1) & 1, "rx_ready set");
    check(mmio_read(RXLEN_ADDR) == (uint32_t)FRAME_LEN, "rx_len == frame length");

    bool match = true;
    for (int i = 0; i < FRAME_LEN; i++) {
        if (mmio_read(RXBUF_BASE + i) != frame[i]) {
            printf("    rx_buf[%d] mismatch\n", i);
            match = false;
        }
    }
    check(match, "rx buffer contents match frame");
}

// ---------------- test 3: loopback ----------------
static void test_loopback() {
    printf("test 3: loopback (tx wired to rx)\n");
    start_tx();

    // wire our own outputs back into our inputs before every edge:
    // the rx fsm sees each nibble one cycle after tx emits it, which
    // is exactly how a real phy repeats it
    std::vector<uint8_t> nibs;
    int expected = 16 + 2 * FRAME_LEN;
    while ((int)nibs.size() < expected) {
        top->mii_rxd = top->mii_txd;
        top->mii_rx_dv = top->mii_tx_en;
        tick();
        if (top->mii_tx_en)
            nibs.push_back(top->mii_txd);
    }
    // one more edge with tx_en high: rx consumes the final nibble
    top->mii_rxd = top->mii_txd; top->mii_rx_dv = top->mii_tx_en;
    tick();
    // dv drops now: rx finalizes on the next edge
    top->mii_rxd = 0; top->mii_rx_dv = 0;
    tick();

    // read everything before any further clock edge
    check((mmio_read(STATUS_ADDR) >> 1) & 1, "rx_ready set after loopback");
    check(mmio_read(RXLEN_ADDR) == (uint32_t)FRAME_LEN, "rx_len == tx_len");
    bool match = true;
    for (int i = 0; i < FRAME_LEN; i++) {
        if (mmio_read(RXBUF_BASE + i) != frame[i]) {
            printf("    loopback byte %d mismatch (got %02X want %02X)\n",
                   i, mmio_read(RXBUF_BASE + i), frame[i]);
            match = false;
        }
    }
    check(match, "loopback: received frame identical to transmitted frame");
}

// ---------------- test 4: mdio smoke ----------------
static void test_mdio() {
    printf("test 4: mdio smoke (busy lifecycle)\n");
    // push some mdio frame and go. the mdio clock is clk/1000 (divider
    // 500 per half), so a 32-bit frame takes ~32000 clk cycles.
    mmio_write(MDIO_CTRL_ADDR, 0xA0000000);

    bool saw_busy = false;
    long cleared_at = -1;
    for (long c = 0; c < 40000; c++) {
        tick();
        uint32_t s = mmio_read(STATUS_ADDR);
        if (s & 4) saw_busy = true;
        else if (saw_busy) { cleared_at = c; break; }
    }
    check(saw_busy, "mdio_busy went high after go");
    check(cleared_at > 0, "mdio_busy cleared after 32 bits");
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    Verilated::traceEverOn(true);
    top = new Vethernet;

    tfp = new VerilatedVcdC;
    top->trace(tfp, 99);
    tfp->open("ethernet.vcd");

    // power-on state: reset held through a few edges
    top->clk = 0; top->reset = 1;
    top->addr = 0; top->data_in = 0; top->wr_en = 0; top->rd_en = 0;
    top->mii_rxd = 0; top->mii_rx_dv = 0;
    top->mii_tx_clk = 0; top->mii_rx_clk = 0;   // unused by the dut, tied off
    for (int i = 0; i < 5; i++) tick();
    top->reset = 0;
    tick();

    test_tx();
    test_rx();
    test_loopback();
    test_mdio();

    tfp->close();
    printf("\n%s (%d failures)\n", failures ? "SOME TESTS FAILED" : "ALL TESTS PASSED", failures);
    delete top;
    return failures ? 1 : 0;
}
