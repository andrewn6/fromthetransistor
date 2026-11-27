// user/kernel mode on the real cpu. needs --uart.
// the kernel half sets up pages and traps, then drops into user_main.
// user code can't touch the uart, so it prints through a syscall.
#include "mmu.h"
#include "trap.h"

#define UART_DATA 0x40001000
#define UART_STAT 0x40001004

#define PT 0x00104000
#define UPAGE 0x0010C000
#define KPAGE 0x0010D000
#define USER_SP 0x007F8000

#define SYS_PUTS 1
#define SYS_MODE 2
#define SYS_EXIT 3

int faults;
int last_far;
int last_fsr;

// ---- kernel ----

void uputc(int c) {
    int *stat = (int *)UART_STAT;
    int *data = (int *)UART_DATA;
    while ((*stat & 1) == 0) { }
    *data = c;
}

void uputs(char *s) {
    while (*s) {
        uputc(*s);
        s++;
    }
}

void uflush() {
    int *stat = (int *)UART_STAT;
    while ((*stat & 1) == 0) { }
}

int on_syscall(int n, int a, int b, int c) {
    if (n == SYS_PUTS) {
        uputs((char *)a);
        return 0;
    }
    if (n == SYS_MODE)
        return get_spsr() & PSR_MODE;
    if (n == SYS_EXIT) {
        if (a == 0)
            uputs("USER OK\n");
        else
            uputs("USER FAIL\n");
        uflush();
        halt();
    }
    return -1;
}

void on_fault(int is_data, int *frame) {
    int *far = (int *)MMU_FAR;
    int *fsr = (int *)MMU_FSR;
    int *pt = (int *)PT;

    faults = faults + 1;
    last_far = *far;
    last_fsr = *fsr;
    if (last_far == KPAGE)
        mmu_map(pt, KPAGE, KPAGE, PTE_PRESENT | PTE_WRITE | PTE_USER);
    else
        frame[13] = frame[13] + 4;   // can't fix it: skip the access
    mmu_flush();
}

void map_range(int *pt, int base, int flags) {
    int i;
    for (i = 0; i < 16; i++)
        mmu_map(pt, base + i * PAGE_SIZE, base + i * PAGE_SIZE, flags);
}

// ---- user ----

int fails;

void check(char *name, int got, int want) {
    if (got == want) {
        syscall(SYS_PUTS, (int)"ok   ", 0, 0);
    } else {
        syscall(SYS_PUTS, (int)"FAIL ", 0, 0);
        fails = fails + 1;
    }
    syscall(SYS_PUTS, (int)name, 0, 0);
    syscall(SYS_PUTS, (int)"\n", 0, 0);
}

void user_part2() {
    check("user: movs pc can't reach kernel mode", get_cpsr() & PSR_MODE, MODE_USR);
    syscall(SYS_EXIT, fails, 0, 0);
}

void user_main() {
    int *up = (int *)UPAGE;
    int *kp = (int *)KPAGE;
    int *uart = (int *)UART_DATA;

    check("user: cpsr says user", get_cpsr() & PSR_MODE, MODE_USR);
    check("user: kernel saw the trap come from user", syscall(SYS_MODE, 0, 0, 0), MODE_USR);
    check("user: read a user page", up[0], 5);

    kp[0] = 9;
    check("user: kernel page faulted", faults, 1);
    check("user: fsr says kernel only", last_fsr, FSR_KERNEL);
    check("user: retried after the kernel allowed it", kp[0], 9);

    *uart = 'X';
    check("user: mmio faulted", faults, 2);
    check("user: far is the uart", last_far, UART_DATA);

    // try to raise itself: msr spsr is ignored, movs pc, lr just jumps
    enter_mode((int)&user_part2, USER_SP - 1024, MODE_SVC);
}

int main() {
    int *pt = (int *)PT;
    int *up = (int *)UPAGE;
    int i;

    faults = 0;
    fails = 0;
    if ((get_cpsr() & PSR_MODE) == MODE_SVC)
        uputs("ok   kernel: boots in svc mode\n");
    else
        uputs("FAIL kernel: boots in svc mode\n");

    for (i = 0; i < PT_ENTRIES; i++)
        pt[i] = 0;
    map_range(pt, 0x00010000, PTE_PRESENT | PTE_WRITE | PTE_USER);  // code + globals
    map_range(pt, 0x007F0000, PTE_PRESENT | PTE_WRITE | PTE_USER);  // stack
    map_range(pt, 0x00100000, PTE_PRESENT | PTE_WRITE);             // kernel only
    mmu_map(pt, UPAGE, UPAGE, PTE_PRESENT | PTE_WRITE | PTE_USER);
    up[0] = 5;

    on_svc((int)&on_syscall);
    mmu_on_abort((int)&on_fault);
    mmu_switch(pt);
    mmu_enable();

    enter_mode((int)&user_main, USER_SP, MODE_USR);
    return 0;
}
