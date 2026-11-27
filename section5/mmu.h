#define MMU_CTRL 0x40006000
#define MMU_TTBR 0x40006004

#define PTE_PRESENT 1
#define PTE_WRITE 2 
#define PTE_USER 4 

#define PAGE_SIZE 4096
#define PT_ENTRIES 4096

void mmu_map(int *pt, int va, int pa, int flags) {
    pt[(va >> 12) & 0xFFF] = (pa & 0xFFFFF000) | flags;
}

void mmu_switch(int *pt) {
    int *ttbr = (int *)MMU_TTBR;
    *ttbr = (int)pt;
}

void mmu_flush() {
    int *ttbr = (int *)MMU_TTBR;
    *ttbr = *ttbr;
}

void mmu_enable() {
    int *ctrl = (int *)MMU_CTRL;
    *ctrl = 1;
}

void mmu_disable() {
    int *ctrl = (int *)MMU_CTRL;
    *ctrl = 0;
}

// fault info, valid in an abort handler
#define MMU_FAR 0x40006008
#define MMU_FSR 0x4000600C
#define FSR_RANGE 1
#define FSR_NOT_PRESENT 2
#define FSR_READ_ONLY 3
#define FSR_KERNEL 4

// the runtime's abort stubs call the function stored here, as
// handler(1, frame) for a data abort, handler(0, frame) for a prefetch
// abort. frame is the saved r0-r12, lr; on return the access is retried
// at frame[13] - 8 (data) or - 4 (prefetch), so frame[13] += 4 skips it.
#define ABORT_FN 0x00010034

void mmu_on_abort(int fn) {
    int *slot = (int *)ABORT_FN;
    *slot = fn;
}
