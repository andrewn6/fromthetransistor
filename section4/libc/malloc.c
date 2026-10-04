#include "libc.h"

#define HEAP_SIZE (64 * 1024) // 64 kib heap arena 
#define ALIGN 8

typedef struct block {
  size_t size;
  int free;
  struct block *next;
} block_t;

static uint8_t heap[HEAP_SIZE];
static block_t *free_list = NULL;

static size_t align_up(size_t n) {
  return (n + (ALIGN - 1)) & ~((size_t)(ALIGN - 1));
}

static void heap_init(void) {
  free_list = (block_t *)heap;
  free_list->size = HEAP_SIZE - sizeof(block_t);
  free_list->free = 1;
  free_list->next = NULL;
}

// malloc returns a pointer to at least n aligned bytes, or NULL on failure.
void *malloc(size_t n) {
  if (n == 0)
    return NULL;
  if (free_list == NULL)
    heap_init();

  n = align_up(n);

  // start at free_list, follow each next pointer until end
  // find a usable block, is the block free and at least n bytes?
  // Split if theres a leftover, if bigger than needed, chop
  // mark it used and return it, flag block as not-free and hand back a pointer to memory
  for (block_t *b = free_list; b != NULL; b = b->next) {
    if (b->free && b->size >= n) {
      // split only if the remainder can hold a header + some payload.
      if (b->size >= n + sizeof(block_t) + ALIGN) {
        block_t *rest = (block_t *)((uint8_t *)(b + 1) + n);
        rest->size = b->size - n - sizeof(block_t);
        rest->free = 1;
        rest->next = b->next;
        b->size = n;
        b->next = rest;
      }
      b->free = 0;
      return (void *)(b + 1);
    }
  }
  return NULL; // out of memory
}

void free(void *p) {
  if (p == NULL)
    return;

  block_t *b = (block_t *)p - 1;
  b->free = 1;

  if (b->next != NULL && b->next->free) {
    b->size += sizeof(block_t) + b->next->size;
    b->next = b->next->next; 
  }
}
