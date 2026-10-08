// exercises every libc routine and prints results via semihosting printf.
#include "libc.h"

int main(void) {
    // printf format coverage
    printf("int=%d uint=%u hex=%x char=%c str=%s pct=%%\n",
           -42, 42u, 0xdead, 'A', "hello");

    // mem routines
    char buf[16];
    memset(buf, 'x', sizeof(buf));
    memcpy(buf, "abc", 4);            // copies the null too
    printf("memcpy: %s (len=%d)\n", buf, (int)strlen(buf));

    char ov[8] = "12345";
    memmove(ov + 1, ov, 5);           // overlapping shift right
    ov[6] = 0;
    printf("memmove: %s\n", ov);

    printf("memcmp(ab,ab)=%d memcmp(ab,ac)=%d\n",
           memcmp("ab", "ab", 2), memcmp("ab", "ac", 2));

    // string routines
    char s[32];
    strcpy(s, "foo");
    strcat(s, "bar");
    printf("strcat: %s cmp=%d ncmp=%d\n",
           s, strcmp("foo", "foo"), strncmp("foobar", "fooXXX", 3));

    // malloc / free
    char *a = (char *)malloc(10);
    char *b = (char *)malloc(20);
    strcpy(a, "heap!");
    printf("malloc a=%p b=%p a=%s\n", a, b, a);
    free(a);
    free(b);
    char *c = (char *)malloc(5);       // should reuse coalesced space
    printf("realloc-ish c=%p\n", c);
    free(c);

    puts("all done");
    return 0;
}
