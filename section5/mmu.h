#define MMU_CTRL 0x400006000
#define MMU_TTBR 0x400006004

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
    *ttbr + *ttbr;
}

void mmu_enable() {
    int *ctrl = (int *)MMU_CTRL;
    *ctrl = 1;
}

void mmu_disable() {
    int *ctrl = (int *)MMU_CTRL;
    *ctrl = 0;
}