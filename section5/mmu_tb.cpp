// mmu alone, tb plays cpu + sram.
// verilator --cc section5/mmu.v --exe section5/mmu_tb.cpp --top-module mmu -Mdir obj_dir_mmu -o mmu_sim
#include "Vmmu.h"
#include <cstdio>
#include <cstdint>

static const uint32_t CTRL = 0x40006000, TTBR = 0x40006004;
static const uint32_t PT = 0x00100000;

static uint32_t mem[1 << 20];
static Vmmu *m;
static int fails = 0, checks = 0;

static uint32_t &word(uint32_t a) { return mem[(a >> 2) & 0xFFFFF]; }

static void settle() {
    for (int k = 0; k < 3; k++) {
        m->mem_rdata = word(m->pa);
        m->eval();
    }
}

static void tick() {
    settle();
    if (m->mem_we) word(m->pa) = m->mem_wdata;
    m->clk = 1; m->eval();
    m->clk = 0; m->eval();
    settle();
}

static void drive(uint32_t va, int we = 0, uint32_t wd = 0) {
    m->va = va; m->cpu_we = we; m->cpu_wdata = wd; m->mem_req = 1;
    settle();
}

// waits out a walk like the stalled cpu
static uint32_t access(uint32_t va, int we = 0, uint32_t wd = 0) {
    drive(va, we, wd);
    while (m->stall) tick();
    uint32_t r = m->cpu_rdata;
    tick();
    return r;
}

static void check(const char *name, uint32_t got, uint32_t want) {
    checks++;
    bool ok = got == want;
    if (!ok) fails++;
    printf("%-30s got %08x want %08x  %s\n", name, got, want, ok ? "ok" : "FAIL");
}

static void map(uint32_t va, uint32_t pa, uint32_t flags) {
    word(PT + ((va >> 12) & 0xFFF) * 4) = (pa & 0xFFFFF000) | flags;
}

int main() {
    m = new Vmmu;
    m->user = 0;
    m->reset = 1; m->eval(); m->reset = 0; m->eval();

    drive(0x00012344);
    check("off: pa == va", m->pa, 0x00012344);
    check("off: no stall", m->stall, 0);
    check("off: no fault", m->fault, 0);

    map(0x00010000, 0x00020000, 3);
    map(0x00011000, 0x00021000, 1);  // read-only
    map(0x00030000, 0x00022000, 7);  // user ok
    map(0x00031000, 0x00023000, 3);  // same slot as 0x11000
    word(0x00020008) = 0xdeadbeef;

    access(TTBR, 1, PT);
    access(CTRL, 1, 1);
    check("ctrl: ttbr readback", access(TTBR), PT);
    check("ctrl: enable readback", access(CTRL), 1);

    drive(0x00010008);
    check("miss: stalls", m->stall, 1);
    check("miss: pa is the pte", m->pa, PT + 0x10 * 4);
    tick();
    check("hit: no stall", m->stall, 0);
    check("hit: pa translated", m->pa, 0x00020008);
    check("hit: data", m->cpu_rdata, 0xdeadbeef);
    tick();

    access(0x0001000c, 1, 0x1234);
    check("store: lands at pa", word(0x0002000c), 0x1234);
    check("store: va untouched", word(0x0001000c), 0);

    // walk cycle must not write the store onto the pte
    access(TTBR, 1, PT);
    access(0x00010010, 1, 0x55);
    check("store miss: pte intact", word(PT + 0x10 * 4), 0x00020003);
    check("store miss: data at pa", word(0x00020010), 0x55);
    access(0x00011000);

    drive(0x00011000, 1, 5);
    check("ro store: fault", m->fault, 1);
    check("ro store: mem_we masked", m->mem_we, 0);
    tick();
    check("ro store: memory unchanged", word(0x00021000), 0);

    access(0x00040000);
    drive(0x00040000);
    check("not present: fault", m->fault, 1);
    check("not present: no re-walk", m->stall, 0);
    tick();

    drive(0x01000000);
    check("above 16MB: fault", m->fault, 1);
    check("above 16MB: no walk", m->stall, 0);
    tick();

    m->user = 1;
    drive(0x00010008);
    check("user on kernel page: fault", m->fault, 1);
    tick();
    access(0x00030000);
    drive(0x00030000);
    check("user on user page: ok", m->fault, 0);
    tick();
    m->user = 0;

    drive(0x40001004);
    check("mmio: pa == va", m->pa, 0x40001004);
    check("mmio: no stall", m->stall, 0);
    check("mmio: no fault", m->fault, 0);
    tick();

    drive(TTBR, 1, PT);
    check("ctrl write: mem_we masked", m->mem_we, 0);
    tick();

    m->mem_req = 0; m->va = 0x00050000; settle();
    check("no mem_req: no walk", m->stall, 0);
    tick();

    access(0x00011000);
    access(0x00031000);
    drive(0x00011000);
    check("conflict: evicted, re-walks", m->stall, 1);
    tick(); tick();

    access(0x00010008);
    access(TTBR, 1, PT);
    drive(0x00010008);
    check("ttbr write: flushes tlb", m->stall, 1);
    tick(); tick();

    map(0x00010000, 0x00022000, 3);
    drive(0x00010008);
    check("stale pte: old frame", m->pa, 0x00020008);
    tick();
    access(TTBR, 1, PT);
    access(0x00010008);
    drive(0x00010008);
    check("after flush: new frame", m->pa, 0x00022008);
    tick();

    access(CTRL, 1, 0);
    drive(0x00010008);
    check("disabled: pa == va", m->pa, 0x00010008);
    check("disabled: no stall", m->stall, 0);

    if (fails) printf("FAIL: %d of %d checks\n", fails, checks);
    else printf("ALL %d CHECKS PASS\n", checks);
    delete m;
    return fails ? 1 : 0;
}
