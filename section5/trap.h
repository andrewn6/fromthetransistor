#define SVC_FN 0x00010038
#define IRQ_FN 0x0001003C
#define PSR_I 0x80
#define TIMER_LOAD 0x40007000
#define TIMER_COUNT 0x40007004
#define TIMER_STATUS 0x40007008

void on_irq(int fn) {
    int *slot = (int *)IRQ_FN;
    *slot = fn;
}

void irq_enable() {
    set_cpsr(get_cpsr() & ~PSR_I);
}

void irq_disable() {
    set_cpsr(get_cpsr() | PSR_I);
}

void on_svc(int fn) {
    int *slot = (int *)SVC_FN;
    *slot = fn;
}
// psr mode field, as get_cpsr() / get_spsr() return it
#define PSR_MODE 0x1F
#define MODE_USR 0x10
#define MODE_SVC 0x13
