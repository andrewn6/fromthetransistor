#define ETH_BASE 0x40002000
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

// note: no volatile - our compiler has no optimizer, nothing gets
// hoisted or cached, so plain pointers are safe for mmio access
void eth_send(char *buf, int len) {
    int *status = (int *)ETH_STATUS;
    int *txlen = (int *)ETH_TXLEN;
    int *ctrl = (int *)ETH_CTRL;
    char *txbuf = (char *)ETH_TXBUF;
    int i;

    while (*status & ETH_ST_TXBUSY) { }

    for (i = 0; i < len; i++) 
        txbuf[i] = buf[i];
    *txlen = len;
    *ctrl = ETH_CTRL_TX;
}

int eth_recv(char *buf, int maxlen) {
    int *status = (int *)ETH_STATUS;
    int *rxlen  = (int *)ETH_RXLEN;
    int *ctrl   = (int *)ETH_CTRL;
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