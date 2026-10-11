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
  return 0;
}
