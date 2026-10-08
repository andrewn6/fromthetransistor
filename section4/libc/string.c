#include "libc.h"

// return number of bytes before terminating null 
size_t strlen(const char *s) {
  size_t n = 0;
  while (s[n])
    n++;
  return n;
}

// compare two strings; unsigned char values decide the sign.
int strcmp(const char *a, const char *b) {
  while (*a && (*a == *b)) {
    a++;
    b++;
  }
  return (int)(uint8_t)*a - (int)(uint8_t)*b;
}

// bounded to at most n characters
int strncmp(const char *a, const char *b, size_t n) {
  for (size_t i = 0; i < n; i++) {
    if (a[i] != b[i])
      return (int)(uint8_t)a[i] - (int)(uint8_t)b[i];
    if (a[i] == 0)
      return 0;
  }
  return 0;
}

char *strcpy(char *dst, const char *src) {
  char *d = dst;
  while ((*d++ = *src++))
    ;
  return dst;
}

// copies up to n bytes null padding if src is shorter 
char *strncpy(char *dst, const char*src, size_t n) {
  size_t i = 0;
  for (; i < n && src[i]; i++)
    dst[i] = src[i];
  for (; i < n; i++)
    dst[i] = 0;
  return dst;
}

char *strcat(char *dst, const char*src) {
  char *d = dst + strlen(dst);  
    while ((*d++ = *src++))
      ;
    return dst;
}
