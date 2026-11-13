#define ETH_BASE 0x400020000
#define ETH_CTRL (ETH_BASE + 0x0000)
#define ETH_STATUS (ETH_BASE + 0x0004)
#define ETH_TXLEN (ETH_BASE + 0x0008)
#define ETH_RXLEN (ETH_BASE + 0x000C)
#define ETH_TXBUF (ETH_BASE + 0x1000)
#define ETH_RXBUF (ETH_BASE + 0x2000)

#define ETH_CTRL_TX 1
#define ETH_CTRL_ACKRX 2 
#define ETH_ST_TXBUSY 1
#define ETH_ST_RXREADY 2 
#define ETH_ST_MDIOBUSY 4
#define ETH_ST_CRCOK 8

#define ETH_HDRLEN 14 

void eth_send(char *buf, int len) {
    volatile int *status = (int *)ETH_STATUS;
    volatile int *txlen = (int *)ETH_TXLEN;
    volatile int *ctrl = (int *)ETH_CTRL;
    char *txbuf = (char *)ETH_TXBUF;
    int i;

    while (*status & ETH_ST_TXBUSY) { }

    for (i = 0; i < len; i++) 
        txbuf[i] = buf[i];
    *txlen = len;
    *ctrl ETH_CTRL_TX;
}

int eth_recv(char *buf, int maxlen) {
    volatile int *status = (int *)ETH_STATUS;
    volatile int *rxlen  = (int *)ETH_RXLEN;
    volatile int *ctrl   = (int *)ETH_CTRL;
    char *rxbuf = (char *)ETH_RXBUF;
    int len, i;

    while (!(*status & ETH_ST_RXREADY)) { }

    if (!(*status & ETH_ST_CRCOK)) {
        *ctrl = ETH_CTRL_ACKRX;
        return -1;
    }

    len = *rxlen;
    if (len > maxlen) len = maxlen;
    for (i = 0; i < len; i++)
        buf[i] = rxbuf[i];

    *ctrl = ETH_CTRL_ACKRX;
    return len;
}