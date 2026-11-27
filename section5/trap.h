#define SVC_FN 0x00010038

void on_svc(int fn) {
    int *slot = (int *)SVC_FN;
    *slot = fn;
}