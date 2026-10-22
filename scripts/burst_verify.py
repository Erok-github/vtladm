#!/usr/bin/env python3
"""Readback verifier for burst_writer.py payloads.

Expects record i at offset i*64KB to be the 8-byte LE index repeated to
64 KB.  Reports the first differing byte as record / absolute offset /
in-record offset plus surrounding hex, so a lost/duplicated/shifted
record vs. torn content is distinguishable from one number
(first-differ at a record boundary and everything after wrong =>
lost/duplicated record; first-differ mid-record with correct index
prefix => content corruption).

Usage: burst_verify.py [device [/dev/st0]] [records [12432]]
       (record count default = 768MiB / 64KiB)
"""
import sys

CHUNK = 64 * 1024


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "/dev/st0"
    maxrec = int(sys.argv[2]) if len(sys.argv) > 2 else (768 * 1024 * 1024) // CHUNK

    bad = 0
    checked = 0
    with open(path, "rb", buffering=0) as f:
        for i in range(maxrec):
            b = f.read(CHUNK)
            if not b:
                break
            if len(b) < CHUNK:
                print(f"SHORT_READ block={i} got={len(b)} (EOD mid-record)")
                break
            expect = i.to_bytes(8, "little") * (CHUNK // 8)
            if b != expect:
                bad += 1
                if bad == 1:
                    for j in range(CHUNK):
                        if b[j] != expect[j]:
                            lo = max(0, j - 8)
                            print(f"FIRST_DIFFER block={i} "
                                  f"abs_offset={i * CHUNK + j} in_record={j}")
                            print(f"  got  {b[lo:j + 8].hex()}")
                            print(f"  want {expect[lo:j + 8].hex()}")
                            break
            checked += 1
    print(f"burst_verify: checked={checked} bad_records={bad}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
