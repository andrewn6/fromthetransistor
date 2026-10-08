#ifndef LIBC_H
#define LIBC_H 

typedef unsigned char uint8_t;
typedef signed char int8_t;
typedef unsigned short uint16_t;
typedef signed short int16_t;
typedef unsigned int uint32_t;
typedef signed int int32_t;
typedef unsigned long long uint64_t;
typedef signed long long int64_t;

typedef unsigned int size_t;
typedef int ssize_t;
typedef unsigned int uintptr_t;

#define NULL ((void*)0)


// Memory  
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
void *memset(void *dst, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
// Strings

size_t strlen(const char *s);
int    strcmp(const char *a, const char *b);
int    strncmp(const char *a, const char *b, size_t n);
char  *strcpy(char *dst, const char *src);
char  *strncpy(char *dst, const char *src, size_t n);
char  *strcat(char *dst, const char *src);

// Heap
void *malloc(size_t n);
void free(void *p);

int putchar(int c);
int puts(const char *s);
int printf(const char *fmt, ...);

void _exit(int code);
#endif
