// talks to the soc.v uart over mmio instead of semihosting.
//   0x40001000  DATA    write: send a byte, read: last received byte
//   0x40001004  STATUS  bit0 = tx_ready, bit1 = rx_valid
// turn on "uart mmio" in the cpu tab, run, then type into the console.
// 'q' quits.

void uart_putc(int c) {
  int *status;
  char *data;
  status = 0x40001004;
  data = 0x40001000;
  while ((*status & 1) == 0) {
  }
  *data = c;
}

int uart_getc() {
  int *status;
  char *data;
  status = 0x40001004;
  data = 0x40001000;
  while ((*status & 2) == 0) {
  }
  return *data;
}

void uart_puts(char *s) {
  while (*s) {
    uart_putc(*s);
    s++;
  }
}

int main() {
  int c;
  uart_puts("uart echo, q to quit\n> ");
  while (1) {
    c = uart_getc();
    uart_putc(c);
    if (c == 'q') {
      break;
    }
  }
  uart_puts("\nbye\n");
  return 0;
}
