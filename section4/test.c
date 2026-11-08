struct point { int x; int y; };
struct block_t { int size; char data[512]; };
struct outer { char c; struct point p; struct point *pp; };

typedef struct block_t blk_t;
typedef int size_t;

int fib(int n) {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}

void itoa(int n, char *buf) {
  int len;
  int i;
  int v;
  if (n == 0) {
    buf[0] = '0';
    buf[1] = 0;
    return;
  }
  len = 0;
  v = n;
  if (n < 0) {
    v = -n;
  }
  while (v > 0) {
    v = v / 10;
    len = len + 1;
  }
  i = len;
  if (n < 0) {
    buf[0] = '-';
    i = i + 1;
  }
  buf[i] = 0;
  v = n;
  if (v < 0) {
    v = -v;
  }
  while (v > 0) {
    i = i - 1;
    buf[i] = '0' + v % 10;
    v = v / 10;
  }
}

int main() {
  char *s;
  int x;
  s = "                    ";
  x = fib(10);
  itoa(x, s);
  putstr(s);
  putstr("\n");
  x = 17 / 5;
  itoa(x, s);
  putstr(s);
  putstr(" ");
  x = 17 % 5;
  itoa(x, s);
  putstr(s);
  putstr("\n");
  s = "hello from our compiler\n";
  putstr(s);
  x = s[0];
  if (x == 104) {
    putstr("index load ok\n");
  }
  x = 0;
  for (int i = 1; i <= 10; i = i + 1) {
    x = x + i;
  }
  itoa(x, s);
  putstr(s);
  putstr("\n");

  x = 0;
  for (int i = 0; i < 100; i = i + 1) {
    if (i == 7) {
      break;
    }
    if (i % 2 == 0) {
      continue;
    }
    x = x + i;
  }
  itoa(x, s);
  putstr(s);
  putstr("\n");

  int i; 
  i = 0;
  x = 0; 
  while (i < 10) {
    i = i + 1;
    if (i == 3) {
      continue;
    }
    if (i == 8) {
      break;
    }
    x = x + i;
  }
  itoa(x, s);
  putstr(s);
  putstr("\n");

  x = 0;
  for (int a = 0; a < 3; a = a + 1) {
    for (int b = 0; b < 10; b = b + 1) {
      if (b == 2) {
        break;
      }
      if (b == 1) {
        continue;
      }
      x = x + 1;
    }
  }

  itoa(x, s);
  putstr(s);
  putstr("\n");

  // compound assignment: (((10+5-3)*2)/5)%3 -> 1
  x = 10;
  x += 5;
  x -= 3;
  x *= 2;
  x /= 5;
  x %= 3;
  itoa(x, s);
  putstr(s);
  putstr("\n");

  // x++ / x-- as expressions yield the old value
  int y;
  x = 1;
  y = x++;
  itoa(y, s);
  putstr(s);
  putstr(" ");
  itoa(x, s);
  putstr(s);
  putstr(" ");
  y = x--;
  itoa(y, s);
  putstr(s);
  putstr(" ");
  itoa(x, s);
  putstr(s);
  putstr("\n");

  // p += 2 skips two chars, *p++ yields 'l' (108)
  x = 99;
  for (int i = 0; i < 10; i++) {
    if (i == 2) {
      break;
    }
  }
  char *p;
  p = "hello";
  p += 2;
  x = *p++;
  itoa(x, s);
  putstr(s);
  putstr("\n");

  // structs: member access via . -> and nested
  struct point pt;
  struct point *q;
  struct outer o;
  pt.x = 3;
  pt.y = 4;
  q = &pt;
  q->x = q->x + 10;
  itoa(q->x, s);
  putstr(s);
  putstr(" ");
  itoa(pt.y, s);
  putstr(s);
  putstr("\n");

  o.c = 1;
  o.p.x = 5;
  o.p.y = 6;
  o.pp = q;
  o.pp->y = 40;
  itoa(o.p.x * 100 + o.p.y, s);
  putstr(s);
  putstr(" ");
  itoa(pt.y, s);
  putstr(s);
  putstr("\n");

  // struct array member: blk.data[0..1] = 'h','i'
  struct block_t blk;
  blk.size = 7;
  blk.data[0] = 'h';
  blk.data[1] = 'i';
  blk.data[2] = 0;
  itoa(blk.size, s);
  putstr(s);
  putstr(" ");
  putstr(blk.data);
  putstr("\n");

  // typedef + sizeof: 4 + 1 + 4 + 512 + 516 + 4 = 1041
  blk_t tb;
  blk_t *tbp;
  size_t n;
  tbp = &tb;
  n = sizeof(int);
  n = n + sizeof(char);
  n = n + sizeof(int *);
  n = n + sizeof tb.data;
  n = n + sizeof *tbp;
  n = n + sizeof(tbp->size);
  itoa(n, s);
  putstr(s);
  putstr("\n");
  itoa(sizeof(blk_t), s);
  putstr(s);
  putstr("\n");

  // unsigned: div/mod, unsigned compare, const parses fine
  unsigned int ua = 12;
  unsigned int ub = 3;
  int r = 0;
  ua = ua / ub;                  // 4
  ua = ua % ub;                  // 1
  if (ua > 0) {
    r = r + 1;                   // unsigned compare true
  }
  const int ci = 2;
  r = r + ci;                    // 3
  ua = 3000000000;
  if (ua > 100) {
    r = r + 10;                  // needs unsigned compare (signed sees < 0)
  }
  ua = ua / 1000000000;          // unsigned div: 3
  itoa(r * 10 + ua, s);          // r = 13 -> 133
  putstr(s);
  putstr("\n");

  return 0;
}
