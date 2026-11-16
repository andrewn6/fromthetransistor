#include "eth.h"

#define DOWNLOAD_ADDR 0x00020000
#define BOOT_ETYPE 0xB007
#define ACK_ETYPE 0xB008
#define BOOT_HDRLEN 22

int rd32(char *b) {
    return (b[0] & 0xFF) 
        | ((b[1] & 0xFF) << 8)
        | ((b[2] & 0xFF) << 16)
        | ((b[3] & 0xFF) << 24);
}

void wr32(char *b, int v) {
    b[0] = v & 0xFF;
    b[1] = (v >> 8) & 0xFF;
    b[2] = (v >> 16) & 0xFF;
    b[3] = (v >> 24) & 0xFF;
}

void send_ack(int want) {
    char buf[18];   

    buf[0] = 0xFF; buf[1] = 0xFF; buf[2] = 0xFF;
    buf[3] = 0xFF; buf[4] = 0xFF; buf[5] = 0xFF;
    buf[6] = 0xAA; buf[7] = 0xBB; buf[8] = 0xCC;
    buf[9] = 0xDD; buf[10] = 0xEE; buf[11] = 0xFF;
    buf[12] = (ACK_ETYPE >> 8) & 0xFF;
    buf[13] = ACK_ETYPE & 0xFF;

    wr32(buf + 14, want);

    eth_send(buf, 18);
}

int main(void) {
    char frame[1536];
    char *dst;
    int len, flen, off, total, have, i;
    char b_hi, b_lo;
    
    b_hi = (BOOT_ETYPE >> 8) & 0xFF;
    b_lo = BOOT_ETYPE & 0xFF;

    dst = DOWNLOAD_ADDR;
    have = 0;
    total = -1;

    while (total < 0 || have < total) {
        len = eth_recv(frame, 1536);

        if (len < BOOT_HDRLEN + 4) continue;
        if (frame[12] != b_hi) continue;
        if (frame[13] != b_lo) continue;

        off = rd32(frame + 14);
        total = rd32(frame + 18);

        if (off == have) {
            flen = len - BOOT_HDRLEN - 4;
            for (i = 0; i < flen; i++) 
                dst[off + i] = frame[BOOT_HDRLEN + i];
            have = off + flen;
        }

        send_ack(have);
    }

    // image complete: jump. jump_to is a runtime asm helper, just
    // bx r0 - the same jump bootrom.asm did. sp is sane because the
    // runtime set it up before main
    jump_to(DOWNLOAD_ADDR);

    while (1) { }   // not reached unless the program returns
}