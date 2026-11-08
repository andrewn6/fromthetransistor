int sum(int n) {
    int s = 0;
    int i = 0;
    while (i < n) {
        s += i;
        i += 1;
    }
    return s;
}
