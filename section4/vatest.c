#define ANSWER 42
#define GREET "from a macro\n"

#include "vatlib.c"

// adapted printf for our dialect: no switch, no casts, no static.
// max fmt + 3 varargs (r0-r3).
int printf(const char *fmt, ...) {
  int ap;
  const char *p;
  va_start(ap, fmt);
  p = fmt;
  while (*p) {
    if (*p != '%') {
      putchar(*p);
      p = p + 1;
      continue;
    }
    p = p + 1;
    if (*p == 'd') {
      out_int(va_arg(ap, int));
    } else if (*p == 'u') {
      out_uint(va_arg(ap, unsigned int), 10, 0);
    } else if (*p == 'x') {
      out_uint(va_arg(ap, unsigned int), 16, 0);
    } else if (*p == 'c') {
      putchar(va_arg(ap, int));
    } else if (*p == 's') {
      out_str(va_arg(ap, const char *));
    } else if (*p == '%') {
      putchar('%');
    }
    p = p + 1;
  }
  va_end(ap);
  return 0;
}

// varargs sum: label + 3 anonymous args = exactly r0-r3
int sum3(const char *label, ...) {
  int ap;
  int t;
  int i;
  va_start(ap, label);
  t = 0;
  for (i = 0; i < 3; i++) {
    t = t + va_arg(ap, int);   // va_arg inside a loop: runtime index
  }
  va_end(ap);
  printf("sum3(%s): ", label);
  return t;
}

int main() {
  int r;
  printf("hello %s! %d %u\n", "varargs", -7, 3000000000);
  printf("hex=%x char=%c%%\n", 48879, 'Q');
  r = sum3("abc", 10, 20, 30);
  printf("%d\n", r);
  printf("macro=%d ", ANSWER);
  printf(GREET);
  r = ANSWER + 1;
  printf("next=%d\n", r);
  return 0;
}
