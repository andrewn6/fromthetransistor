import argparse
import socket 
import struct 
import sys 
import zlib

BOOT_ETYPE = 0xB007
ACK_ETYPE = 0xB008
ETH_HDR = 14
BOOT_HDR = 8 
SPLIT = 1400 # payload bytes per fragment
ACK_TIMEOUT = 0.5

DST = bytes([0xFF] * 6)
SRC = bytes([0x11, 0x22, 0x33, 0x44, 0x55, 0x66])

def fcs(data: bytes) -> bytes:
    # crc32 == the 802.3 fcs 
    # computes the crc32 checksum using XOR polynomial divison to detect data corruption
    # it checks if received data and crc produce a zero remainder after division
    return zlib.crc32(data).to_bytes(4, "little")

def fragment(image: bytes, off: int, split: int = SPLIT) -> bytes:
    body = image[off : off + split]
    hdr = (
        DST
        + SRC 
        + struct.pack(">H", BOOT_ETYPE)
        + struct.pack("<I", off)
        + struct.pack("<I", len(image))
    )
    return hdr + body + fcs(hdr + body)

def send(iface: str, path: str) -> int: 
    image = open(path, "rb").read()
    if not image:
        sys.exit("empty image")
    try:
        s = socket.socket(socket.AF_PACKET, socket.SOCK_RAW,
                            socket.htons(ACK_ETYPE))
    except (AttributeError, OSError) as e:
        sys.exit(f"raw sockets unavailable here (AF_PACKET is linux_only): {e}")
    s.bind((iface, 0))
    s.settimeout(ACK_TIMEOUT)

    have = 0
    sends = 0 
    print(f"sending {len(image)} bytes to {iface} in {SPLIT}-byte fragments")
    while have < len(image):
        s.send(fragment(image, have))
        sends += 1 
        try:
            frame, _ = s.recvfrom(2048)
        except socket.timeout:
            print(f". no ack at {have}, resending")
            continue 
        if len (frame) < ETH_HDR + 4:
            continue  
        etype = struct.unpack("<I", frame[14:18])[0]
        if etype != ACK_ETYPE:
            continue 
        want = struct.unpack("<I", frame[14:18])[0]
        if want > have:
            print(f" ack: {have} -> {want} ({len(image) - want} to go)")
            have = want 
    print(f"done: {len(image)} bytes, {sends} fragment sends")
    return 0

def selftest() -> int:
    image = bytes.fromhex("2a00a0e3 0110a0e3 011aa0e1 000081e5 563412ef")
    frag0 = fragment(image, 0, 10)
    frag0 = fragment(image, 0, 10)
    frag1 = fragment(image, 10, 10)
    ok = True 

    def check(cond, what):
        nonlocal ok 
        print((" PASS: " if cond else " FAIL: ")+ what)
        ok = ok and cond 

    check(len(frag0) == 36, "fragment length: 22 hdr + 10 payload + 4 fcs")
    check(frag0[12:14] == bytes([0xB0, 0x07]), "ethertype 0xB007, big-endian at byte 12")
    check(struct.unpack("<I", frag0[14:18])[0] == 0, "offset 0, little-endian at byte 14")
    check(struct.unpack("<I", frag0[18:22])[0] == 20, "total 20, little-endian at byte 18")
    check(struct.unpack("<I", frag1[14:18])[0] == 10, "fragment 1 carries offset 10")
    for name, frag in (("frag0", frag0), ("frag1", frag1)):
        body = frag[:-4]
        check(frag[-4:] == zlib.crc32(body).to_bytes(4, "little"),
              f"{name}: fcs = zlib crc32, little-endian")
        check(zlib.crc32(frag) == 0x2144DF1C,
              f"{name}: good-frame residue 0x2144DF1C")
    print("SELFTEST " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1

def main() -> int:
    p = argparse.ArgumentParser(
        description="send an image to the ethernet bootloader over raw ethernet")
    p.add_argument("iface", nargs="?", help="network interface, e.g. eth0")
    p.add_argument("image", nargs="?", help="flat binary to send")
    p.add_argument("--selftest", action="store_true",
                   help="verify frame construction without a nic")
    args = p.parse_args()

    if args.selftest:
        return selftest()
    if not args.iface or not args.image:
        p.error("iface and image required (or use --selftest)")
    return send(args.iface, args.image)


if __name__ == "__main__":
    sys.exit(main())