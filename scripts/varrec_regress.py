#!/usr/bin/env python3
"""R1 regression: variable-length records + cross-record large reads
(field issue P0-1, fixed 2026-10-23).

Phases:
  write        seed-42 variable-size records (16K..112K in 4K steps)
               to /dev/nst0, deterministic per-record content
  verify-exact replay the same size sequence with request == record
               (must hash-match; proves the write path + cursor)
  verify-fullblock  assemble 256K blocks via read(remaining), dd
               iflag=fullblock style (request shrinks below record
               sizes). Real-tape semantics: the first record larger
               than the remainder returns ILI/ENOMEM — a LOUD failure.
               A silently wrong stream means the P0-1 truncation
               regressed -> exit 1.

Usage: varrec_regress.py [write|verify-exact|verify-fullblock]
       (write rewinds + erases first; the verify phases rewind via mt(1))
"""
import hashlib
import os
import random
import subprocess
import sys

SEED = 42
TOTAL = 767887360
CHUNK_MAX = 114688
BS = 256 * 1024


def sizes():
    random.seed(SEED)
    out, t = [], 0
    while t < TOTAL:
        n = min(random.randrange(16384, CHUNK_MAX, 4096), TOTAL - t)
        out.append(n)
        t += n
    return out


def payload(i, n):
    d = hashlib.sha256(str(i).encode()).digest()
    return (d * (n // len(d) + 1))[:n]


def rewind():
    subprocess.run(["mt", "-f", "/dev/st0", "rewind"], check=True)


def erase():
    subprocess.run(["mt", "-f", "/dev/st0", "erase"], check=True)


def cmd_write():
    # nst0 is a NO-REWIND device: a bare open() leaves the position wherever
    # the previous session stopped and the records would land mid-tape,
    # after older experiment data.  Field incident 2026-10-23: verify-exact
    # then read stale cat|dd 64K fragments from BOT and mis-reported
    # "first bad record 0 got=65536" against a brand-new write.  Start from
    # a clean, positioned medium.
    rewind()
    erase()
    sz = sizes()
    with open("/dev/nst0", "wb", buffering=0) as f:
        for i, n in enumerate(sz):
            f.write(payload(i, n))
    print(f"R1 wrote {len(sz)} records / {sum(sz)} bytes")


def cmd_verify_exact():
    rewind()
    sz = sizes()
    bad = 0
    fd = os.open("/dev/st0", os.O_RDONLY)
    try:
        for i, n in enumerate(sz):
            b = os.read(fd, n)
            if b != payload(i, n):
                bad += 1
                if bad == 1:
                    print(f"R1_EXACT first bad record {i} got={len(b)}")
    finally:
        os.close(fd)
    print(f"R1_EXACT {'OK' if bad == 0 else 'FAIL'} "
          f"({len(sz)} records, bad={bad})")
    return 1 if bad else 0


def cmd_verify_fullblock():
    rewind()
    src = hashlib.sha256()
    for i, n in enumerate(sizes()):
        src.update(payload(i, n))
    fd = os.open("/dev/st0", os.O_RDONLY)
    got = hashlib.sha256()
    done = 0
    try:
        while done < TOTAL:
            want = min(BS, TOTAL - done)
            buf = b""
            while len(buf) < want:
                try:
                    b = os.read(fd, want - len(buf))
                except OSError as e:
                    # Expected real-tape semantics: ENOMEM from ILI with a
                    # negative information field (record > remaining).
                    print(f"R1_FULLBLOCK loud error at stream offset "
                          f"{done + len(buf)}: errno={e.errno} "
                          f"({'EXPECTED' if e.errno == 12 else 'UNEXPECTED'})")
                    print("R1_FULLBLOCK OK (loud failure, no silent "
                          "corruption)" if e.errno == 12
                          else "R1_FULLBLOCK FAIL (wrong errno)")
                    return 0 if e.errno == 12 else 1
                if not b:
                    print(f"R1_FULLBLOCK FAIL: EOF at {done + len(buf)}")
                    return 1
                buf += b
            got.update(buf)
            done += want
    finally:
        os.close(fd)
    if got.digest() == src.digest():
        print("R1_FULLBLOCK OK (stream complete and hash-matched)")
        return 0
    print("R1_FULLBLOCK FAIL: silent wrong stream (P0-1 regression)")
    return 1


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "write"
    if cmd == "write":
        cmd_write()
        sys.exit(0)
    if cmd == "verify-exact":
        sys.exit(cmd_verify_exact())
    if cmd == "verify-fullblock":
        sys.exit(cmd_verify_fullblock())
    print(__doc__)
    sys.exit(2)
