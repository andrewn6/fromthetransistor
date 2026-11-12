// ethernet_tb.cpp - verilator testbench for ethernet.v
//
// the tb plays both sides of the chip:
//   - the CPU: drives addr/data_in/wr_en on the MMIO bus
//   - the PHY: drives mii_rxd/mii_rx_dv, watches mii_txd/mii_tx_en
//
// the dut runs its tx/rx state machines on mii_tx_clk/mii_rx_clk and crosses
// into the cpu clock domain through synchronizers, like real silicon, so
// this tb generates those clocks too. they run at the cpu clock rate here:
// the ratio does not matter, only that the domains are separate.
//
// tests:
//   1. tx:        capture the wire nibbles, verify preamble+sfd,
//                 low-nibble-first payload and a correct LE crc32 fcs
//   2. rx:        feed a frame with a good fcs, verify rx_ready holds until
//                 the cpu acks, buffer + crc_ok through mmio
//   3. loopback:  wire tx -> rx, get the identical frame back, crc ok
//   4. rx bad:    corrupted fcs lands, crc_ok clear, driver drops it
//   5. mdio:      busy lifecycle
//
// build:
//   verilator --trace --cc ethernet.v --exe ethernet_tb.cpp --top-module ethernet
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
    CTRL_ADDR     = 0x0000,   // bit0 = start tx, bit1 = ack rx
    STATUS_ADDR   = 0x0004,   // bit0 tx_busy, bit1 rx_ready, bit2 mdio_busy, bit3 rx_crc_ok
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

// eval with waveform dump. time always advances, so dumps never collide
// and the "previous dump at t=X" warnings never fire
static void eval() {
    top->eval();
    if (tfp) tfp->dump(sim_time);
    sim_time++;
}

// one cpu clock with both mii clocks rising alongside it. reads see
// pre-edge values, everything scheduled lands at the end of the eval,
// exactly like the old single-clock tb but with the domains split
static void tick() {
    top->clk = 0; top->mii_tx_clk = 0; top->mii_rx_clk = 0; eval();
    top->clk = 1; top->mii_tx_clk = 1; top->mii_rx_clk = 1; eval();
}

static void mmio_write(uint32_t a, uint32_t v) {
    top->addr = a; top->data_in = v;
    top->wr_en = 1; top->rd_en = 0;
    tick();                      // captured at this posedge
    top->wr_en = 0; top->addr = 0; top->data_in = 0;
}

// reads are combinational in the dut (always_comb mux), no clock edge needed
static uint32_t mmio_read(uint32_t a) {
    top->addr = a; top->rd_en = 1; top->wr_en = 0;
    eval();
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

// zlib-style crc32 = the 802.3 fcs crc. the dut appends the crc value
// little-endian after the payload and checks that the running register
// over payload+fcs lands on 0xDEBB20E3 (= 0x2144DF1C with final xor)
static uint32_t crc_tab[256];
static void crc_init() {
    for (uint32_t i = 0; i < 256; i++) {
        uint32_t c = i;
        for (int k = 0; k < 8; k++)
            c = (c & 1) ? (c >> 1) ^ 0xEDB88320u : c >> 1;
        crc_tab[i] = c;
    }
}
static uint32_t crc32(const uint8_t* d, int n) {
    uint32_t c = 0xFFFFFFFFu;
    for (int i = 0; i < n; i++)
        c = crc_tab[(c ^ d[i]) & 0xFF] ^ (c >> 8);
    return c ^ 0xFFFFFFFFu;
}

// payload with its 4 fcs bytes appended, little-endian
static void frame_with_fcs(std::vector<uint8_t>& v) {
    v.insert(v.end(), frame, frame + FRAME_LEN);
    uint32_t c = crc32(frame, FRAME_LEN);
    for (int i = 0; i < 4; i++) v.push_back((c >> (8 * i)) & 0xFF);
}

// load the frame into the tx buffer and hit go
static void start_tx() {
    for (int i = 0; i < FRAME_LEN; i++)
        mmio_write(TXBUF_BASE + i, frame[i]);
    mmio_write(TXLEN_ADDR, FRAME_LEN);
    mmio_write(CTRL_ADDR, 1);
}

// pretend to be the phy: preamble, sfd, frame bytes low nibble first
static void feed_frame(const std::vector<uint8_t>& bytes) {
    auto nib = [&](uint8_t n) {
        top->mii_rxd = n; top->mii_rx_dv = 1;
        tick();
    };
    for (int i = 0; i < 14; i++) nib(0x5);   // preamble
    nib(0x5); nib(0xD);                       // sfd
    for (size_t i = 0; i < bytes.size(); i++) {
        nib(bytes[i] & 0xF);
        nib(bytes[i] >> 4);
    }
    top->mii_rx_dv = 0;
    tick();                                   // rx finalizes on this edge
    // rx_done crosses into the cpu domain through the toggle sync:
    // give it a few clocks before rx_ready shows up in STATUS
    for (int c = 0; c < 5; c++) tick();
}

// verify a captured nibble stream against frame+fcs: 14 preamble nibbles,
// sfd (5 then D), then each byte low nibble first
static bool check_wire_order(const std::vector<uint8_t>& nibs) {
    std::vector<uint8_t> full;
    frame_with_fcs(full);

    if (nibs.size() != 16 + 2 * full.size()) {
        printf("    expected %d nibbles, captured %zu\n",
               16 + 2 * (int)full.size(), nibs.size());
        return false;
    }
    for (int i = 0; i < 14; i++)
        if (nibs[i] != 0x5) { printf("    preamble nibble %d != 5\n", i); return false; }
    if (nibs[14] != 0x5) { printf("    sfd low nibble != 5\n"); return false; }
    if (nibs[15] != 0xD) { printf("    sfd high nibble != D\n"); return false; }
    for (size_t i = 0; i < full.size(); i++) {
        uint8_t lo = full[i] & 0xF, hi = full[i] >> 4;
        if (nibs[16 + 2*i] != lo || nibs[17 + 2*i] != hi) {
            printf("    byte %zu nibbles wrong (got %X %X want %X %X)\n",
                   i, nibs[16 + 2*i], nibs[17 + 2*i], lo, hi);
            return false;
        }
    }
    return true;
}

// ---------------- test 1: transmit ----------------
static void test_tx() {
    printf("test 1: tx (cpu -> phy)\n");
    start_tx();

    // capture every nibble while tx_en is high: 16 preamble/sfd + payload + fcs
    std::vector<uint8_t> nibs;
    int expected = 16 + 2 * (FRAME_LEN + 4);
    for (int c = 0; c < 3000 && (int)nibs.size() < expected; c++) {
        tick();
        if (top->mii_tx_en)
            nibs.push_back(top->mii_txd);
    }
    // let the ipg finish and the busy flag sync across
    for (int c = 0; c < 60; c++) tick();

    check(check_wire_order(nibs), "wire order: preamble, sfd, payload+fcs low nibble first");
    check((mmio_read(STATUS_ADDR) & 1) == 0, "tx_busy cleared after ipg");
}

// ---------------- test 2: receive ----------------
static void test_rx() {
    printf("test 2: rx (phy -> cpu)\n");
    std::vector<uint8_t> full;
    frame_with_fcs(full);
    feed_frame(full);

    uint32_t status = mmio_read(STATUS_ADDR);
    check((status >> 1) & 1, "rx_ready set after frame");
    check((status >> 3) & 1, "rx_crc_ok set");
    check(mmio_read(RXLEN_ADDR) == (uint32_t)full.size(), "rx_len == payload + fcs");

    bool match = true;
    for (size_t i = 0; i < full.size(); i++) {
        if (mmio_read(RXBUF_BASE + i) != full[i]) {
            printf("    rx_buf[%zu] mismatch\n", i);
            match = false;
        }
    }
    check(match, "rx buffer contents match frame + fcs");

    // ready must hold until we ack it, not vanish on its own. the clear
    // lands on the posedge AFTER the ack write, so give it a couple of
    // clocks - a real cpu has those between the store and the next poll
    mmio_write(CTRL_ADDR, 0b10);
    for (int c = 0; c < 3; c++) tick();
    check(((mmio_read(STATUS_ADDR) >> 1) & 1) == 0, "rx_ready cleared by cpu ack");
}

// ---------------- test 3: loopback ----------------
static void test_loopback() {
    printf("test 3: loopback (tx wired to rx)\n");
    start_tx();

    std::vector<uint8_t> full;
    frame_with_fcs(full);
    int expected = 16 + 2 * (int)full.size();

    // wire our own outputs back into our inputs before every edge: the rx
    // fsm sees each nibble one mii cycle after tx emits it, which is
    // exactly how a real phy repeats it
    std::vector<uint8_t> nibs;
    for (int c = 0; c < 3000 && (int)nibs.size() < expected; c++) {
        top->mii_rxd = top->mii_txd;
        top->mii_rx_dv = top->mii_tx_en;
        tick();
        if (top->mii_tx_en)
            nibs.push_back(top->mii_txd);
    }
    // one more edge with tx_en still high: rx consumes the final nibble
    top->mii_rxd = top->mii_txd; top->mii_rx_dv = top->mii_tx_en;
    tick();
    // tx is in ipg now, so dv drops: rx finalizes on this edge
    top->mii_rxd = 0; top->mii_rx_dv = 0;
    tick();
    for (int c = 0; c < 5; c++) tick();   // let rx_done sync across

    check(check_wire_order(nibs), "wire capture: whole frame on the wire");
    check((mmio_read(STATUS_ADDR) >> 1) & 1, "rx_ready set after loopback");
    check((mmio_read(STATUS_ADDR) >> 3) & 1, "loopback frame passes crc");
    check(mmio_read(RXLEN_ADDR) == (uint32_t)full.size(), "rx_len == tx payload + fcs");

    bool match = true;
    for (size_t i = 0; i < full.size(); i++) {
        if (mmio_read(RXBUF_BASE + i) != full[i]) {
            printf("    loopback byte %zu mismatch (got %02X want %02X)\n",
                   i, mmio_read(RXBUF_BASE + i), full[i]);
            match = false;
        }
    }
    check(match, "loopback: received frame identical to transmitted frame");
    mmio_write(CTRL_ADDR, 0b10);   // ack before the next test
}

// ---------------- test 4: bad crc ----------------
static void test_rx_bad() {
    printf("test 4: rx bad crc (corrupted fcs)\n");
    std::vector<uint8_t> bad;
    frame_with_fcs(bad);
    bad.back() ^= 0xFF;             // flip the last fcs byte
    feed_frame(bad);

    uint32_t status = mmio_read(STATUS_ADDR);
    check((status >> 1) & 1, "rx_ready set, frame still delivered");
    check(!((status >> 3) & 1), "rx_crc_ok clear, driver must drop it");
    mmio_write(CTRL_ADDR, 0b10);
}

// ---------------- test 5: mdio smoke ----------------
static void test_mdio() {
    printf("test 5: mdio smoke (busy lifecycle)\n");
    // st=01, op=01 (write), phyad=0, reg=0: a write frame drives all 32
    // bits and never releases the line. read data gets verified against a
    // real phy model another day
    mmio_write(MDIO_CTRL_ADDR, 0x50000000);

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
    crc_init();

    tfp = new VerilatedVcdC;
    top->trace(tfp, 99);
    tfp->open("ethernet.vcd");

    // power-on state: reset held through a few edges
    top->clk = 0; top->reset = 1;
    top->addr = 0; top->data_in = 0;
    top->wr_en = 0; top->rd_en = 0;
    top->mii_rxd = 0; top->mii_rx_dv = 0;
    top->mii_tx_clk = 0; top->mii_rx_clk = 0;
    for (int i = 0; i < 5; i++) tick();
    top->reset = 0;
    tick();

    test_tx();
    test_rx();
    test_loopback();
    test_rx_bad();
    test_mdio();

    tfp->close();
    printf("\n%s (%d failures)\n", failures ? "SOME TESTS FAILED" : "ALL TESTS PASSED", failures);
    delete top;
    return failures ? 1 : 0;
}
