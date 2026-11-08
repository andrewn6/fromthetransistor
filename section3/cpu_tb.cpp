// cpu_tb.cpp - verilator harness for arm7_cpu
//
// loads a flat image (build.py's *.elf.bin) at the linked base and runs
// until the cpu halts on svc, then checks cputest.c's results area.
//
// the cpu resets to pc = 0x10000 but the toolchain links text at
// 0x10084 (after the elf headers), so the harness plants a one-word
// trampoline branch at 0x10000 pointing at the entry.
//
// memory map (rest is unmapped, fatal on touch):
//   0x00010000 + 1MB   sram: image at 0x10054, results at 0x00100000
//   0x007f0000 + 64KB  stack page below the runtime's sp = 0x00800000
//
// build:
//   verilator --cc arm7_cpu.v --exe cpu_tb.cpp --top-module arm7_cpu \
//     -Mdir obj_dir_cpu -o cpu_sim
//   make -C obj_dir_cpu -f Varm7_cpu.mk
// run:
//   ./obj_dir_cpu/cpu_sim ../section4/build/cputest.bin 0x10054
//   (build.py prints the exact cpu_sim line when it makes the .bin)
#include <verilated.h>
#include "Varm7_cpu.h"
#include <cstdio>
#include <cstdint>
#include <cstring>

static const uint32_t SRAM_BASE = 0x00010000, SRAM_SIZE = 0x00100000;
static const uint32_t STK_BASE  = 0x007F0000, STK_SIZE  = 0x00010000;
static const uint32_t RESULTS   = 0x00100000;

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

int main(int argc, char** argv) {
  Verilated::commandArgs(argc, argv);
  if (argc != 3) {
    printf("usage: cpu_sim image.bin entry(hex, e.g. 0x10084)\n");
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

  Varm7_cpu* top = new Varm7_cpu;
  top->clk = 0;
  top->reset = 1;
  top->eval();
  // one edge under reset so the cpu latches pc/phase/flags
  top->clk = 1;
  top->eval();
  top->clk = 0;
  top->eval();
  top->reset = 0;

  uint64_t cycles = 0;
  while (!top->halted && cycles < 20000000) {
    // present memory for the access the cpu wants this cycle. byte
    // reads put the addressed byte in lane 0 (the cpu only uses [7:0]).
    if (top->sram_byte) {
      top->sram_data_in = *mem_ptr(top->sram_addr, 1, "read");
    } else {
      top->sram_data_in = read32(top->sram_addr);
    }
    // capture the write request before the edge consumes it
    uint32_t wa = top->sram_addr;
    uint32_t wd = top->sram_data_out;
    bool we = top->sram_we;
    bool wb = top->sram_byte;

    top->clk = 1;
    top->eval();
    top->clk = 0;
    top->eval();

    if (we) {
      if (wb) *mem_ptr(wa, 1, "write") = wd & 0xFF;
      else write32(wa, wd);
    }
    cycles++;
  }

  if (!top->halted) {
    printf("FAIL: timeout after %llu cycles (cpu never hit svc)\n",
           (unsigned long long)cycles);
    return 1;
  }
  printf("halted on svc after %llu cycles\n", (unsigned long long)cycles);

  // expected values from cputest.c
  static const uint32_t expected[] = {
    55, 5050, 3, 2, 3000, 34, 104105, 25, 218,
  };
  int fails = 0;
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

  top->final();
  delete top;
  return fails ? 1 : 0;
}
