#define SVC_FN 0x00010038

void on_svc(int fn) {
    int *slot = (int *)SVC_FN;
    *slot = fn;
}
// psr mode field, as get_cpsr() / get_spsr() return it
#define PSR_MODE 0x1F
#define MODE_USR 0x10
#define MODE_SVC 0x13
