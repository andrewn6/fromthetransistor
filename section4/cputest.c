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
// R[9] = 255     |= : 0xF0 | 0x0F
// R[10] = 240    ^= : 0xFF ^ 0x0F
// R[11] = 1024   <<= : 1 << 10
// R[12] = 64     >>= : 0x1000 >> 6 (signed, asr)
// R[13] = -4     -16 >> 2, arithmetic shift
// R[14] = 8      0x80000000u >> 28, logical shift
// R[15] = 40     5 << n, variable shift count (register form)
// R[16] = 65280  ~0xFF & 0xFFFF
// R[17] = 0xF0000000  0x80000000 >> 3, variable asr

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
  unsigned int bu;
  int i;
  int t;
  int n;
  int bm;
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

  // bitwise + shifts
  bm = 0xF0;
  bm |= 0x0F;
  R[9] = bm;
  bm = 0xFF;
  bm ^= 0x0F;
  R[10] = bm;
  bm = 1;
  bm <<= 10;
  R[11] = bm;
  bm = 0x1000;
  bm >>= 6;
  R[12] = bm;
  bm = -16;
  bm >>= 2;
  R[13] = bm;
  bu = 0x80000000;
  R[14] = bu >> 28;
  n = 3;
  R[15] = 5 << n;
  R[16] = ~0xFF & 0xFFFF;
  R[17] = 0x80000000 >> n;

  return 0;
}
