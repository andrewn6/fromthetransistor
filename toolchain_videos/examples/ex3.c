int g = 42;
char msg[6] = "hello";

int main() {
    putstr("hi\n");
    int a[3];
    a[0] = 7;
    a[1] = g + a[0];
    putstr(msg);
    return a[1] / 2;
}
