// support routines for vatest.c; putchar itself lives in the runtime
void out_str(const char *s) {
  while (*s) {
    putchar(*s);
    s++;
  }
}

void out_uint(unsigned int v, unsigned int base, int upper) {
  char buf[32];
  const char *digits;
  int i;
  if (upper) {
    digits = "0123456789ABCDEF";
  } else {
    digits = "0123456789abcdef";
  }
  i = 0;
  if (v == 0) {
    putchar('0');
    return;
  }
  while (v) {
    buf[i] = digits[v % base];
    i++;
    v = v / base;
  }
  while (i > 0) {
    i = i - 1;
    putchar(buf[i]);
  }
}

void out_int(int v) {
  if (v < 0) {
    putchar('-');
    v = -v;
  }
  out_uint(v, 10, 0);
}
