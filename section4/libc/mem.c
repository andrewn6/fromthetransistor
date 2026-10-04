#include "libc.h"

void *memcpy(void *dst, const void *src, size_t n) {
  uint8_t *d = (uint8_t *)dst;
  const uint8_t *s = (const uint8_t)src;
  for (size_t i = 0; i < n; i++) 
    d[i] = s[i];
  return dst
}

// this copies n bytes and is safe when the regions overlap
void *memmove(void *dst, const void *src, size_t n) {
  uint8_t *d = (uint8_t *)dst;
  const uint8_t *s = (const uint8_t *)src;
  if (d == s || n == 0) 
    return dst;
  if (d < s) {
    for (size_t i = 0; i < n; i++)
      d[i] = s[i];
  } else {
    for (size_t i = n; i > 0; i--)
      d[i - 1] = s[i - 1];
  }
  return dst;
}

void *memset(void *dst, int c, size_t  n) {
  uint8_t *d = (uint8_t *)dst;
  uint8_t b = (uint8_t)c;
  for (size_t i = 0; i < n; i++)
    d[i] = b;
  return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
  const uint8_t *pa = (const uint8_t *)a;
  const uint8_t *pb = (const uint8_t *)b;
  for (size_t i = 0; i < n; i++) {
    if (pa[i] != pb[i])
      return (int)pa[i] - (int)pb[i];
  }
}
