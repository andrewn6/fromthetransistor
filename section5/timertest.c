// timer interrupt on the real cpu. needs --uart.
#include "trap.h"

#define UART_DATA 0x40001000
#define UART_STAT 0x40001004

#define USER_SP 0x007F8000

int fails;
int ticks;
int user_ticks;

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

void finish() {
    if (fails == 0)
        uputs("TIMER OK\n");
    else
        uputs("TIMER FAIL\n");
    uflush();
    halt();
}

void on_tick(int *frame) {
    int *status = (int *)TIMER_STATUS;
    *status = 1;                          // ack, or it fires again at once
    ticks = ticks + 1;
    if ((get_spsr() & PSR_MODE) == MODE_USR) {
        // the user loop never yields: only the timer gets us back here
        user_ticks = user_ticks + 1;
        if (user_ticks == 3) {
            check("preempt: kernel got control back from a spinning user", 1, 1);
            check("preempt: interrupts were on in user mode", get_spsr() & PSR_I, 0);
            finish();
        }
    }
}

void user_spin() {
    while (1) { }
}

int main() {
    int *load = (int *)TIMER_LOAD;
    int *count = (int *)TIMER_COUNT;
    int i;
    int sum;
    int c1;

    fails = 0;
    ticks = 0;
    user_ticks = 0;
    check("boot: interrupts masked", get_cpsr() & PSR_I, PSR_I);

    on_irq((int)&on_tick);
    *load = 3000;
    c1 = *count;
    check("timer: counting down", c1 < 3000 && c1 > 0, 1);

    // masked: the tick goes pending but nothing is taken
    for (i = 0; i < 1000; i++) { }
    check("masked: no ticks taken", ticks, 0);

    // unmask: the pending tick fires right away, then keep running
    // a loop with lots of compares while ticks land in the middle
    irq_enable();
    check("unmasked: pending tick taken", ticks >= 1, 1);
    sum = 0;
    for (i = 1; i <= 2000; i++)
        sum = sum + i;
    check("ticks land mid-loop without breaking it", sum, 2001000);
    check("more ticks arrived", ticks > 2, 1);

    irq_disable();
    c1 = ticks;
    for (i = 0; i < 2000; i++) { }
    check("disabled again: no new ticks", ticks, c1);

    enter_mode((int)&user_spin, USER_SP, MODE_USR);
    return 0;
}
