#!/usr/bin/env python3
"""osc_capture.py で録った /hokuyohub/cluster の TSV を、元のタイミングで再送する(依存ライブラリなし)。

使い方:
    python3 tools/osc_replay.py testdata/capture2_40s.tsv 11001 [12001 ...]

ポートを複数指定すると、同じパケットを同時に全ポートへ送る。
oF 版と Rust 版の受信ポートを別々にしておけば、同じ入力を並べて比較できる。
同じパケット番号のメッセージが複数あるときは OSC バンドルにまとめて送る。
"""
import socket
import struct
import sys
import time


def pad(b):
    return b + b"\0" * (4 - len(b) % 4)


def osc_string(s):
    return pad(s.encode())


def build_cluster_message(args):
    # 型タグ ,ihiffffffi : id, timestamp(ns), frame, cx, cy, minx, miny, maxx, maxy, points
    cid, ts, frame = int(args[0]), int(args[1]), int(args[2])
    floats = [float(a) for a in args[3:9]]
    points = int(args[9])
    body = struct.pack(">iqi", cid, ts, frame) + struct.pack(">6f", *floats) + struct.pack(">i", points)
    return osc_string("/hokuyohub/cluster") + osc_string(",ihiffffffi") + body


def build_bundle(messages):
    out = osc_string("#bundle") + struct.pack(">Q", 1)  # timetag 1 = immediately
    for m in messages:
        out += struct.pack(">i", len(m)) + m
    return out


def main():
    path = sys.argv[1]
    ports = [int(p) for p in sys.argv[2:]]
    packets = []  # (受信時刻, [メッセージ])
    for line in open(path):
        cols = line.rstrip("\n").split("\t")
        if cols[2] != "/hokuyohub/cluster":
            continue
        t, packet = float(cols[0]), cols[1]
        msg = build_cluster_message(cols[4:])
        if packets and packets[-1][2] == packet:
            packets[-1][1].append(msg)
        else:
            packets.append((t, [msg], packet))
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    t0 = packets[0][0]
    start = time.time()
    for t, msgs, _ in packets:
        wait = (t - t0) - (time.time() - start)
        if wait > 0:
            time.sleep(wait)
        data = msgs[0] if len(msgs) == 1 else build_bundle(msgs)
        for port in ports:
            sock.sendto(data, ("127.0.0.1", port))
    print(f"sent {len(packets)} packets to {ports}", file=sys.stderr)


if __name__ == "__main__":
    main()
