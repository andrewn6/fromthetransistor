// a tour of the dialect our compiler speaks. edit anything: the pipeline
// rebuilds as you type and the cpu tab runs the result.
struct point { int x; int y; };

int square(int n) {
  return n * n;
}

void print_int(int n) {
  char buf[12];
  int i;
  if (n == 0) {
    putchar('0');
    return;
  }
  if (n < 0) {
    putchar('-');
    n = -n;
  }
  i = 0;
  while (n > 0) {
    buf[i] = '0' + n % 10;
    n = n / 10;
    i++;
  }
  while (i > 0) {
    i--;
    putchar(buf[i]);
  }
}

int main() {
  struct point p;
  int i;
  p.x = 3;
  p.y = 4;
  putstr("hello from the transistor\n");
  for (i = 1; i <= 5; i++) {
    print_int(square(i));
    putchar(' ');
  }
  putchar('\n');
  print_int(p.x * p.x + p.y * p.y);
  putstr(" = 3*3 + 4*4\n");
  return 0;
}
