#include "trap.h"

#define UART_DATA 0x40001000
#define UART_STAT 0x40001004

int fails;
int calls;

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
        uputs("ok ");
    } else {
        uputs("FAIL ");
        fails = fails + 1;
    }
    uputs(name);
    uputc('\n');
}

int handler(int n, int a, int b, int c) { 
    calls = calls + 1;
    if (n == 1) 
        return a + b + c;
    if (n == 2) {
        uputs(" (hello from inside a syscall)\n");
        return 0;
    }
    return -2;
}

int main() {
    int x;
    int y;

    fails = 0;
    calls = 0;
    check("no handler: returns -1", syscall(1, 2, 3, 4), -1);

    on_svc((int)&handler);
    check("args in, result out", syscall(1, 10, 20, 30), 60);
    check("handler ran once", calls, 1);
    check("handler can do io", syscall(2, 0, 0, 0), 0);
    check("unknown number", syscall(9, 0, 0, 0), -2);

    // locals live across the trap
    x = 7;
    y = syscall(1, x, x, x);
    check("locals survive", x, 7);
    check("result with locals", y, 21);

    if (fails == 0)
        uputs("SVC OK\n");
    else
        uputs("SVC FAIL\n");
    uflush();
    return 0;
}