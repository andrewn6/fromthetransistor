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

int main(int argc, char** argv) {
  Verilated::commandArgs(argc, argv);
  bool uart_mode = argc == 5 && strcmp(argv[3], "--uart") == 0;
  if (argc != 3 && !uart_mode) {
    printf("usage: cpu_sim image.bin entry [--uart text]\n");
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

  Vsoc* top = new Vsoc;
  top->clk = 0;
  top->reset = 1;
  top->uart_rx = 1;
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
    // present memory for the access the cpu wants this cycle. byte
    // reads put the addressed byte in lane 0 (the cpu only uses [7:0]).
    // uart accesses are muxed inside soc.v, so skip them here.
    bool is_uart = (top->mem_addr >> 4) == (0x40001000 >> 4);
    if (is_uart) {
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
