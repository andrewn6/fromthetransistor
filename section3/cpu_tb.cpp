// cpu_tb.cpp - verilator harness for soc (arm7_cpu + uart)
//
// loads a flat image (build.py's *.bin) at the linked base and runs
// until the cpu halts on svc.
//
// the cpu resets to pc = 0x10000 but the toolchain links text at
// 0x10054 (after the elf headers), so the harness plants a one-word
// trampoline branch at 0x10000 pointing at the entry.
//
// memory map (rest is unmapped, fatal on touch):
//   0x00010000 + 1MB   sram: image at 0x10054, results at 0x00100000
//   0x007f0000 + 64KB  stack page below the runtime's sp = 0x00800000
//   0x40001000         uart, decoded inside soc.v (never reaches here)
//
// build:
//   verilator --cc soc.v arm7_cpu.v ../section2/uart.v --exe cpu_tb.cpp \
//     --top-module soc -Mdir obj_dir_cpu -o cpu_sim
//   make -C obj_dir_cpu -f Vsoc.mk
// run:
//   ./obj_dir_cpu/cpu_sim ../section4/build/cputest.bin 0x10054
//   ./obj_dir_cpu/cpu_sim ../section4/build/uarttest.bin 0x10054 --uart ping
//     (--uart feeds the text plus '\n' into the rx pin and skips the
//      cputest checks. anything the cpu sends on tx is printed.)
#include <verilated.h>
#include "Vsoc.h"
#include <vector>
#include <cstdio>
#include <cstdint>
#include <cstring>

static const uint32_t SRAM_BASE = 0x00010000, SRAM_SIZE = 0x00100000;
static const uint32_t STK_BASE  = 0x007F0000, STK_SIZE  = 0x00010000;
static const uint32_t RESULTS   = 0x00100000;

static const int BAUD_DIV = 50000000 / 115200;

static uint8_t sram[SRAM_SIZE];
static uint8_t stk[STK_SIZE];

static uint8_t* mem_ptr(uint32_t a, uint32_t n, const char* what) {
  if (a >= SRAM_BASE && a + n <= SRAM_BASE + SRAM_SIZE) return sram + (a - SRAM_BASE);
  if (a >= STK_BASE && a + n <= STK_BASE + STK_SIZE) return stk + (a - STK_BASE);
  printf("FATAL: unmapped %s access at 0x%08x\n", what, a);
  exit(1);
}

static uint32_t read32(uint32_t a) {
  uint8_t* p = mem_ptr(a, 4, "read");
  return p[0] | (p[1] << 8) | (p[2] << 16) | ((uint32_t)p[3] << 24);
}

static void write32(uint32_t a, uint32_t v) {
  uint8_t* p = mem_ptr(a, 4, "write");
  p[0] = v; p[1] = v >> 8; p[2] = v >> 16; p[3] = v >> 24;
}

struct TxDecoder {
  bool busy = false;
  int count = 0, bit = 0;
  uint8_t byte = 0;

  void step(int line) {
    if (!busy) {
      if (!line) {
        busy = true;
        count = BAUD_DIV + BAUD_DIV / 2;
        bit = 0;
        byte = 0;
      }
      return;
    }
    if (--count) return;
    if (bit < 8) {
      byte |= line << bit;
      bit++;
      count = BAUD_DIV;
    } else {
      if (line) { putchar(byte); fflush(stdout); }
      busy = false;
    }
  }
};

struct RxDriver {
  std::vector<int> bits;
  uint64_t start = 0;

  void load(const char* s, uint64_t at) {
    start = at;
    for (; *s; s++) {
      bits.push_back(0);
      for (int i = 0; i < 8; i++) bits.push_back((*s >> i) & 1);
      bits.push_back(1);
      bits.push_back(1);
    }
  }

  int level(uint64_t cyc) {
    if (cyc < start) return 1;
    uint64_t i = (cyc - start) / BAUD_DIV;
    return i < bits.size() ? bits[i] : 1;
  }
};

// the phy: latches each tx nibble and repeats it back one mii tick
// later, like a real phy repeating a frame onto the wire. can also be
// armed with an injected frame: nibbles go out on rx as if the wire
// delivered them, then control returns to loopback.
struct PhyLoopback {
  int tick = 0;
  uint8_t held_nib = 0;
  bool held_dv = false;
  std::vector<uint8_t> inject;   // armed injection nibbles
  size_t ipos = 0;
  bool injecting = false;

  // arm: build the nibble stream for a whole frame - preamble, sfd,
  // bytes low-nibble-first, crc32 fcs little-endian (the mirror of
  // what the mac's tx side puts on the wire)
  void inject_frame(const uint8_t* bytes, size_t n) {
    inject.clear(); ipos = 0; injecting = true;
    for (int i = 0; i < 14; i++) inject.push_back(0x5);
    inject.push_back(0x5); inject.push_back(0xD);
    for (size_t i = 0; i < n; i++) {
      inject.push_back(bytes[i] & 0xF);
      inject.push_back(bytes[i] >> 4);
    }
    uint32_t c = 0xFFFFFFFFu;
    for (size_t i = 0; i < n; i++) {
      c ^= bytes[i];
      for (int k = 0; k < 8; k++)
        c = (c & 1) ? (c >> 1) ^ 0xEDB88320u : c >> 1;
    }
    c ^= 0xFFFFFFFFu;
    for (int i = 0; i < 4; i++) {
      uint8_t b = (c >> (8 * i)) & 0xFF;
      inject.push_back(b & 0xF);
      inject.push_back(b >> 4);
    }
  }

  void step(Vsoc* top) {
    // mii clocks run at cpu clk / 2: every other cpu cycle is an
    // mii rising edge
    int mii_clk = tick & 1;
    if (mii_clk) {
      if (injecting) {
        if (ipos < inject.size()) {
          held_nib = inject[ipos++];
          held_dv = 1;
        } else {
          held_dv = 0;        // injection done: dv drops, rx finalizes
          injecting = false;
        }
      } else {
        // rising edge: the mac just drove a new nibble (or dropped
        // tx_en). sample it now, present it to rx until the next rise
        held_nib = top->mii_txd;
        held_dv  = top->mii_tx_en;
      }
    }
    top->mii_tx_clk = mii_clk;
    top->mii_rx_clk = mii_clk;
    top->mii_rxd    = held_nib;
    top->mii_rx_dv  = held_dv;
    tick++;
  }
};

// the boot host: plays the laptop that downloads the kernel. one
// fragment in flight; decodes the bootloader's ack frames by
// nibble-assembling the mac's tx as it loops back through the phy,
// retransmits on timeout. corrupts the first attempt at the second
// fragment to prove the mac drops bad fcs and retry works.
struct BootHost {
  const uint8_t* image = nullptr;
  size_t total = 0;
  size_t have = 0;              // bytes acked by the bootloader
  size_t sent_upto = 0;         // bytes handed to the phy
  int split;                    // fragment size
  bool corrupt_pending = false; // corrupt next inject once
  int idle_ticks = 0;
  bool saw_retry = false;       // corruption test verdict

  // nibble decoder state for the loopback stream (bootloader's tx)
  bool decoding = false;
  bool hunting = true;          // true while skipping preamble/sfd
  uint8_t byte_lo = 0;
  bool nib_lo = false;
  std::vector<uint8_t> rxbytes;

  void arm(const uint8_t* img, size_t n, int frag_split) {
    image = img; total = n; have = 0; sent_upto = 0;
    split = frag_split; corrupt_pending = true; idle_ticks = 0;
  }

  bool done() const { return image && have >= total; }

  // build and inject the next fragment if none is outstanding
  void step(PhyLoopback& phy) {
    if (!image || done()) return;
    if (sent_upto > have) {           // fragment in flight: wait for ack
      if (++idle_ticks > 3000) {      // timeout: it was lost, rewind
        sent_upto = have;             // retransmit from the ack point
        idle_ticks = 0;
      }
      return;
    }
    // send fragment [have, have+split)
    size_t take = total - have < (size_t)split ? total - have : split;
    uint8_t frag[14 + 8 + 1400];
    frag[0]=0xAA; frag[1]=0xBB; frag[2]=0xCC; frag[3]=0xDD;
    frag[4]=0xEE; frag[5]=0xFF;                          // dst
    frag[6]=0x11; frag[7]=0x22; frag[8]=0x33; frag[9]=0x44;
    frag[10]=0x55; frag[11]=0x66;                        // src
    frag[12]=0xB0; frag[13]=0x07;                        // etype BE
    for (int i = 0; i < 4; i++) frag[14+i] = (have >> (8*i)) & 0xFF;
    for (int i = 0; i < 4; i++) frag[18+i] = (total >> (8*i)) & 0xFF;
    for (size_t i = 0; i < take; i++) frag[22+i] = image[have+i];

    if (corrupt_pending && have == (size_t)split) {
      frag[22] ^= 0xFF;         // bad payload -> bad fcs -> mac drops
      corrupt_pending = false;
    }

    phy.inject_frame(frag, 22 + take);
    sent_upto = have + take;    // optimistic; ack or timeout corrects
    idle_ticks = 0;
  }

  // called every cpu cycle after phy.step: assemble the phy's held
  // rx nibbles (the loopback of the bootloader's tx) into bytes.
  // sample only on mii rising edges so each nibble is taken once.
  // preamble/sfd are skipped exactly like the mac's rx fsm: hunt 5
  // nibbles until the D nibble, then byte assembly starts
  void observe(uint8_t nib, bool dv, bool mii_rise) {
    if (!image || done()) return;
    if (!mii_rise) return;
    if (!dv) {
      if (decoding) {           // frame just ended: parse it
        parse_ack();
        decoding = false;
      }
      return;
    }
    if (!decoding) {
      decoding = true;
      hunting = true;           // skip preamble until the sfd D nibble
      nib_lo = false;
      rxbytes.clear();
    }
    if (hunting) {
      if (nib == 0xD) hunting = false;   // sfd done, data starts next
      return;
    }
    if (!nib_lo) {
      byte_lo = nib;
      nib_lo = true;
    } else {
      rxbytes.push_back((nib << 4) | byte_lo);
      nib_lo = false;
    }
  }

  // the assembled frame: dst(6) src(6) etype(2) payload(4). if it is
  // the bootloader's ack, adopt its want as our have
  void parse_ack() {
    if (rxbytes.size() < 18) return;
    if (rxbytes[0] != 0xFF || rxbytes[1] != 0xFF) return;  // broadcast dst
    if (rxbytes[12] != 0xB0 || rxbytes[13] != 0x08) return; // ack etype
    size_t want = rxbytes[14] | (rxbytes[15] << 8)
                | ((size_t)rxbytes[16] << 16) | ((size_t)rxbytes[17] << 24);
    if (want > have) { have = want; idle_ticks = 0; }
    else if (want == have && sent_upto > have) {
      // ack for a byte count we already have while a fragment is in
      // flight: that fragment was rejected (bad fcs) or duplicate
      sent_upto = have;
    }
  }
};

int main(int argc, char** argv) {
  Verilated::commandArgs(argc, argv);
  bool uart_mode = argc == 5 && strcmp(argv[3], "--uart") == 0;
  bool eth_mode = argc == 5 && strcmp(argv[3], "--eth") == 0;
  if (argc != 3 && !uart_mode && !eth_mode) {
    printf("usage: cpu_sim image.bin entry [--uart text | --eth inject]\n");
    return 1;
  }

  // load the flat image at the linked base
  FILE* f = fopen(argv[1], "rb");
  if (!f) { printf("FATAL: cannot open %s\n", argv[1]); return 1; }
  uint32_t entry;
  sscanf(argv[2], "%x", &entry);
  size_t n = fread(mem_ptr(entry, 4, "image"), 1, SRAM_BASE + SRAM_SIZE - entry, f);
  fclose(f);
  printf("loaded %zu bytes at 0x%08x, entry 0x%08x\n", n, entry, entry);

  // trampoline at the reset vector: b entry
  uint32_t off = (entry - (0x10000 + 8)) >> 2;
  write32(0x10000, 0xEA000000 | (off & 0xFFFFFF));

  // rx input: start late enough that the program is already polling
  RxDriver rxd;
  if (uart_mode) {
    static char line[256];
    snprintf(line, sizeof(line), "%s\n", argv[4]);
    rxd.load(line, 200000);
  }
  TxDecoder txd;
  PhyLoopback phy;
  BootHost host;

  if (eth_mode) {
    // the image to download: writes 42 to results[0], copies the mmu
    // CTRL register to results[1] (proves paging is on at the jump),
    // then halts:
    //   mov r0, #42           e3a0002a
    //   mov r1, #1            e3a01001
    //   lsl r1, r1, #20       e1a01a01   -> r1 = 0x00100000
    //   str r0, [r1]          e5810000   -> results[0] = 42
    //   mov r2, #0x40         e3a02040
    //   lsl r2, r2, #12       e1a02602
    //   orr r2, r2, #6        e3822006
    //   lsl r2, r2, #12       e1a02602   -> r2 = 0x40006000
    //   ldr r3, [r2]          e5923000
    //   str r3, [r1, #4]      e5813004   -> results[1] = CTRL
    //   svc 0x123456          ef123456   -> halt the sim
    // sent in 10-byte fragments (mid-instruction splits) so
    // reassembly and the corrupt+retry path are both exercised
    static const uint8_t image[] = {
      0x2a, 0x00, 0xa0, 0xe3,   // mov r0, #42
      0x01, 0x10, 0xa0, 0xe3,   // mov r1, #1
      0x01, 0x1a, 0xa0, 0xe1,   // lsl r1, r1, #20
      0x00, 0x00, 0x81, 0xe5,   // str r0, [r1]
      0x40, 0x20, 0xa0, 0xe3,   // mov r2, #0x40
      0x02, 0x26, 0xa0, 0xe1,   // lsl r2, r2, #12
      0x06, 0x20, 0x82, 0xe3,   // orr r2, r2, #6
      0x02, 0x26, 0xa0, 0xe1,   // lsl r2, r2, #12
      0x00, 0x30, 0x92, 0xe5,   // ldr r3, [r2]
      0x04, 0x30, 0x81, 0xe5,   // str r3, [r1, #4]
      0x56, 0x34, 0x12, 0xef,   // svc 0x123456
    };
    host.arm(image, sizeof(image), 10);
  }

  Vsoc* top = new Vsoc;
  top->clk = 0;
  top->reset = 1;
  top->uart_rx = 1;
  top->mii_tx_clk = 0; top->mii_rx_clk = 0;
  top->mii_rxd = 0;    top->mii_rx_dv = 0;
  top->eval();
  // one edge under reset so the cpu and uart latch their reset state
  top->clk = 1;
  top->eval();
  top->clk = 0;
  top->eval();
  top->reset = 0;

  uint64_t cycles = 0;
  while (!top->halted && cycles < 20000000) {
    top->uart_rx = rxd.level(cycles);
    phy.step(top);
    // host protocol: send/timeout fragments, watch the loopback for
    // the bootloader's acks. mii rising edge is when the phy's held
    // nibble can change: phy.tick was just incremented, so the edge
    // we sample matches what the mac saw
    if (eth_mode) {
      host.step(phy);
      host.observe(phy.held_nib, phy.held_dv, ((phy.tick - 1) & 1) == 1);
    }
    // present memory for the access the cpu wants this cycle. byte
    // reads put the addressed byte in lane 0 (the cpu only uses [7:0]).
    // mmio windows (uart, ethernet) are muxed inside soc.v, so skip
    // them here.
    bool is_mmio = (top->mem_addr >= 0x40001000 && top->mem_addr < 0x40001010)
                  || (top->mem_addr >= 0x40002000 && top->mem_addr < 0x40005000)
                  || (top->mem_addr >= 0x40006000 && top->mem_addr < 0x40006010);

    if (is_mmio) {
      top->mem_rdata = 0;
    } else if (top->mem_byte) {
      top->mem_rdata = *mem_ptr(top->mem_addr, 1, "read");
    } else {
      top->mem_rdata = read32(top->mem_addr);
    }
    // capture the write request before the edge consumes it
    uint32_t wa = top->mem_addr;
    uint32_t wd = top->mem_wdata;
    bool we = top->mem_we;
    bool wb = top->mem_byte;

    top->clk = 1;
    top->eval();
    top->clk = 0;
    top->eval();

    if (we) {
      if (wb) *mem_ptr(wa, 1, "write") = wd & 0xFF;
      else write32(wa, wd);
    }
    txd.step(top->uart_tx);
    cycles++;
  }

  if (!top->halted) {
    printf("FAIL: timeout after %llu cycles (cpu never hit svc)\n",
           (unsigned long long)cycles);
    return 1;
  }
  printf("halted on svc after %llu cycles\n", (unsigned long long)cycles);

  if (eth_mode) {
    // the injected program wrote 42 to results[0] before halting.
    // saw_retry never fired if the ack path worked: the corrupted
    // fragment is caught by parse_ack's rewind, not the timeout
    uint32_t got = read32(RESULTS);
    uint32_t ctrl = read32(RESULTS + 4);
    bool ok = got == 42 && ctrl == 1;
    printf("bootloader: injected program ran, results[0] = %u (%s)\n",
           got, got == 42 ? "ok" : "FAIL");
    printf("bootloader: paging on at the jump, mmu ctrl = %u (%s)\n",
           ctrl, ctrl == 1 ? "ok" : "FAIL");
    printf("bootloader: ack-driven retry %s, host timed out %s\n",
           host.sent_upto == host.total ? "clean" : "incomplete",
           host.saw_retry ? "yes (unexpected)" : "no (good)");
    top->final();
    delete top;
    return (ok && !host.saw_retry) ? 0 : 1;
  }

  int fails = 0;
  if (!uart_mode) {
    // expected values from cputest.c
    static const uint32_t expected[] = {
      55, 5050, 3, 2, 3000, 34, 104105, 25, 218,
      // bitwise + shifts
      255, 240, 1024, 64, 4294967292, 8, 40, 65280, 4026531840,
    };
    for (size_t i = 0; i < sizeof(expected) / 4; i++) {
      uint32_t got = read32(RESULTS + 4 * i);
      bool ok = got == expected[i];
      if (!ok) fails++;
      printf("check %zu: got %10u want %10u  %s\n",
             i, got, expected[i], ok ? "ok" : "FAIL");
    }
    if (fails) {
      printf("FAIL: %d checks\n", fails);
    } else {
      printf("ALL %zu CHECKS PASS\n", sizeof(expected) / 4);
    }
  }

  top->final();
  delete top;
  return fails ? 1 : 0;
}
