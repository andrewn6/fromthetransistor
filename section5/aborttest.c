#include "mmu.h"

#define UART_DATA 0x40001000
#define UART_STAT 0x40001004

#define PT 0x00104000
#define FRAME1 0x0010C000
#define FRAME2 0x0010D000
#define VDATA 0x00200000
#define VRO 0x00201000
#define VCODE 0x00300000

int fails;
int faults;
int last_kind;
int last_far;
int last_fsr;

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
    while (*s) {
        uputc(*s);
        s++;
    }
}

void check(char *name, int got, int want) {
    if (got == want) {
        uputs("ok   ");
    } else {
        uputs("FAIL ");
        fails = fails + 1;
    }
    uputs(name);
    uputc('\n');
}

int answer() {
    return 42;
}

void on_abort(int is_data) {
    int *far = (int *)MMU_FAR;
    int *fsr = (int *)MMU_FSR;
    int *pt = (int *)PT;
    int code;

    faults = faults + 1;
    last_kind = is_data;
    last_far = *far;
    last_fsr = *fsr;

    if (last_far == VDATA)
        mmu_map(pt, VDATA, FRAME1, PTE_PRESENT | PTE_WRITE);
    if (last_far == VRO)
        mmu_map(pt, VRO, FRAME2, PTE_PRESENT | PTE_WRITE);
    if ((last_far & 0xFFFFF000) == VCODE) {
        code = (int)&answer & 0xFFFFF000;
        mmu_map(pt, VCODE, code, PTE_PRESENT);
        mmu_map(pt, VCODE + PAGE_SIZE, code + PAGE_SIZE, PTE_PRESENT);
    }

    mmu_flush();
}

void map_range(int *pt, int base) {
    int i;
    for (i = 0; i < 16; i++)
        mmu_map(pt, base + i * PAGE_SIZE, base + i * PAGE_SIZE, PTE_PRESENT | PTE_WRITE);
}

int main() {
    int *pt = (int *)PT;
    int *vd = (int *)VDATA;
    int *vro = (int *)VRO;
    int *f1 = (int *)FRAME1;
    int *f2 = (int *)FRAME2;
    int i;
    int r;

    fails = 0;
    faults = 0;
    for (i = 0; i < PT_ENTRIES; i++)
        pt[i] = 0;
    map_range(pt, 0x00010000);
    map_range(pt, 0x00100000);
    map_range(pt, 0x007F0000);
    mmu_map(pt, VRO, FRAME2, PTE_PRESENT);   // read-only

    mmu_on_abort((int)&on_abort);
    mmu_switch(pt);
    mmu_enable();

    vd[0] = 0x1234;
    check("not present: one fault", faults, 1);
    check("not present: data abort", last_kind, 1);
    check("not present: far", last_far, VDATA);
    check("not present: fsr", last_fsr, FSR_NOT_PRESENT);
    check("not present: retried store landed", f1[0], 0x1234);

    f2[0] = 7;
    check("read-only: read ok", vro[0], 7);
    check("read-only: no fault on read", faults, 1);
    vro[0] = 0x5678;
    check("read-only: store faulted", faults, 2);
    check("read-only: fsr", last_fsr, FSR_READ_ONLY);
    check("read-only: retried store landed", f2[0], 0x5678);

    r = jump_to(VCODE + ((int)&answer & 0xFFF));
    check("prefetch: fault", faults, 3);
    check("prefetch: prefetch abort", last_kind, 0);
    check("prefetch: fsr", last_fsr, FSR_NOT_PRESENT);
    check("prefetch: code ran at new va", r, 42);

    vd[1] = 9;
    check("mapped: no fault", faults, 3);

    mmu_disable();
    if (fails == 0)
        uputs("ABORT OK\n");
    else
        uputs("ABORT FAIL\n");
    uflush();
    return 0;
}