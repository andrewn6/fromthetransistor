#include "mmu.h"

#define UART_DATA 0x40001000
#define UART_STAT 0x40001004

#define PT1 0x00104000
#define PT2 0x00108000
#define FRAME1 0x0010C000
#define FRAME2 0x0010D000
#define VPAGE 0x00200000

int fails;

void uputc(int c) {
    int *stat = (int *)UART_STAT;
    int *data = (int *)UART_DATA;
    while ((*stat & 1) == 0) { }
    *data = c;
}

void uflush() {
    int *stat = (int *)UART_STAT;
    while ((*stat & 1) == 0) { }
}

void uputs(char *s) {
    while(*s) {
        uputc(*s);
        s++;
    }
}

void check(char *name, int got, int want) {
    if (got == want) {
        uputs("ok ");
    } else {
        uputs("FAIL ");
        fails = fails + 1;
    }
    uputs(name);
    uputc('\n');
}

void map_range(int *pt, int base) {
    int i;
    for (i = 0; i < 16; i++)
        mmu_map(pt, base + i * PAGE_SIZE, base + i * PAGE_SIZE, PTE_PRESENT | PTE_WRITE);
}

void build(int *pt, int frame) {
    int i;
    for (i = 0; i < PT_ENTRIES; i++)
        pt[i] = 0;
    map_range(pt, 0x00010000);
    map_range(pt, 0x00100000);
    map_range(pt, 0x007F0000);
    mmu_map(pt, VPAGE, frame, PTE_PRESENT | PTE_WRITE);
}

int main() {
    int *pt1 = (int *)PT1;
    int *pt2 = (int *)PT2;
    int *v = (int *)VPAGE;
    int *f1 = (int *)FRAME1;
    int *f2 = (int *)FRAME2;
    int *ttbr = (int *)MMU_TTBR;
    int *ctrl = (int *)MMU_CTRL;

    fails = 0;
    build(pt1, FRAME1);
    build(pt2, FRAME2);

    mmu_switch(pt1);
    mmu_enable();
    check("enabled, still running", 1, 1);
    check("ctrl readback", *ctrl, 1);
    check("ttbr readback", *ttbr, PT1);

    // VPAGE has no physical memory: this only works translated
    v[0] = 0x1234;
    check("remap: write lands in frame1", f1[0], 0x1234);
    check("remap: read back through va", v[0], 0x1234);

    mmu_switch(pt2);
    v[0] = 0x5678;
    check("switch: write lands in frame2", f2[0], 0x5678);
    check("switch: frame1 untouched", f1[0], 0x1234);

    mmu_switch(pt1);
    check("switch back: va sees frame1", v[0], 0x1234);
    mmu_switch(pt2);
    check("switch again: va sees frame2", v[0], 0x5678);

    // edit a live pte, flush, then the va moves
    mmu_switch(pt1);
    mmu_map(pt1, VPAGE, FRAME2, PTE_PRESENT | PTE_WRITE);
    mmu_flush();
    check("edit + flush: va moved", v[0], 0x5678);

    mmu_disable();
    check("disabled, still running", 1, 1);

    if (fails == 0)
        uputs("PAGING OK\n");
    else
        uputs("PAGING FAIL\n");
    uflush();
    return 0;
}