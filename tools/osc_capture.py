#!/usr/bin/env python3
"""UDP で届いた OSC を TSV に書き出す(依存ライブラリなし)。

使い方:
    python3 tools/osc_capture.py <port> <seconds> > out.tsv

出力列(タブ区切り):
    受信時刻(UNIX秒)  パケット番号  アドレス  型タグ  引数...
5秒間何も届かなければ途中で終了する。
"""
import socket
import struct
import sys
import time


def read_string(b, i):
    end = b.index(b"\0", i)
    s = b[i:end].decode()
    i = end + 1
    i += (-i) % 4
    return s, i


def parse(b, out):
    if b.startswith(b"#bundle"):
        i = 16
        while i < len(b):
            n = struct.unpack(">i", b[i:i + 4])[0]
            parse(b[i + 4:i + 4 + n], out)
            i += 4 + n
        return
    addr, i = read_string(b, 0)
    tags, i = read_string(b, i)
    args = []
    for t in tags[1:]:
        if t == "i":
            args.append(struct.unpack(">i", b[i:i + 4])[0]); i += 4
        elif t == "f":
            args.append(struct.unpack(">f", b[i:i + 4])[0]); i += 4
        elif t == "h":
            args.append(struct.unpack(">q", b[i:i + 8])[0]); i += 8
        elif t == "d":
            args.append(struct.unpack(">d", b[i:i + 8])[0]); i += 8
        elif t == "s":
            s, i = read_string(b, i); args.append(s)
    out.append((addr, tags, args))


def main():
    port = int(sys.argv[1])
    seconds = float(sys.argv[2])
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("0.0.0.0", port))
    sock.settimeout(5)
    end = time.time() + seconds
    packet = 0
    while time.time() < end:
        try:
            b, _ = sock.recvfrom(65536)
        except socket.timeout:
            break
        packet += 1
        msgs = []
        parse(b, msgs)
        for addr, tags, args in msgs:
            print(f"{time.time():.6f}\t{packet}\t{addr}\t{tags}\t" + "\t".join(map(str, args)), flush=True)


if __name__ == "__main__":
    main()
