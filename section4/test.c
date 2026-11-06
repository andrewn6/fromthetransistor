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
  
  return 0;
}
