#define UART_DATA 0x40001000 
#define UART_STAT 0x40001004
#define TX_READY 1 
#define RX_VALID 2 

void uputc(int c) {
    int *stat;
    int *data;
    stat = UART_STAT;
    data = UART_DATA;
    while ((*stat & TX_READY) == 0) {
    }
    *data = c;
}

int ugetc() {
    int *stat;
    int *data;
    stat = UART_STAT;
    data = UART_DATA;
    while ((*stat & RX_VALID) == 0){
    }
    return *data;
} 

// we wait until lsat byte has fully left the wire
void uflush() {
    int *stat;
    stat = UART_STAT;
    while ((*stat & TX_READY) == 0) {
    }
}

void uputs(char *s) {
    while (*s) {
        uputc(*s);
        s++;
    }
}

int main() {
    int c;
    uputs("hello from the uart\n");
    uputs("echo: ");
    c = ugetc();
    while (c != '\n') {
        uputc(c);
        c = ugetc();
    }
    uputc('\n');
    uflush();
    return 0;
}