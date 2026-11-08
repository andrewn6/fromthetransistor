// cputest.c - runs on the verilator arm7_cpu (section3/cpu_tb.cpp).
//
// no semihosting here: on the cpu, svc just halts. results go to a fixed
// memory area the testbench checks word by word.
//
// R[0] = 55      fib(10): recursion, bl/lr, push/pop, stack frame
// R[1] = 5050    sum 1..100: for loop
// R[2] = 3       17/5: signed division via __divsi3
// R[3] = 2       17%5: signed modulo via __modsi3
// R[4] = 3000    unsigned division via __udivsi3
// R[5] = 34      struct member access
// R[6] = 104105  byte load/store: 'h'*1000 + 'i'
// R[7] = 25      break/continue inside a loop
// R[8] = 218     global array + absolute addressing: 'o' + 'k'

char gbuf[8];

struct point { int x; int y; };

int fib(int n) {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}

int main() {
  int *R;
  struct point p;
  char buf[16];
  unsigned int u;
  int i;
  int t;
  R = 0x00100000;

  R[0] = fib(10);

  t = 0;
  for (i = 1; i <= 100; i = i + 1) {
    t = t + i;
  }
  R[1] = t;

  R[2] = 17 / 5;
  R[3] = 17 % 5;

  u = 3000000000;
  R[4] = u / 1000000;

  p.x = 3;
  p.y = 4;
  R[5] = p.x * 10 + p.y;

  buf[0] = 'h';
  buf[1] = 'i';
  buf[2] = 0;
  R[6] = buf[0] * 1000 + buf[1];

  t = 0;
  for (i = 0; i < 10; i = i + 1) {
    if (i == 3) {
      continue;
    }
    if (i == 8) {
      break;
    }
    t = t + i;
  }
  R[7] = t;

  gbuf[0] = 'o';
  gbuf[1] = 'k';
  R[8] = gbuf[0] + gbuf[1];

  return 0;
}
