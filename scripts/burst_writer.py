#!/usr/bin/env python3
"""E3 minimal reproducer for the 2026-10-22 "burst small-record write"
incident (vtladm issue list #1): 16 x 64 KB records back-to-back, then
~50 ms pause, repeated.  Streaming writers (cat|dd) do NOT trigger the
race; this burst shape does (three corrupt sessions out of three).

Payload is self-verifying: record i is the 8-byte LE block index
repeated to 64 KB, so the first wrong byte identifies the corrupted
record and offset exactly.  Pair with burst_verify.py for readback.

Usage: burst_writer.py [device [/dev/nst0]] [total-bytes [768MiB]]
"""
import os
import sys
import time

CHUNK = 64 * 1024
BURST = 16
PAUSE_S = 0.05


def block_bytes(i: int) -> bytes:
    return i.to_bytes(8, "little") * (CHUNK // 8)


def main() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else "/dev/nst0"
    total = int(sys.argv[2]) if len(sys.argv) > 2 else 768 * 1024 * 1024

    written = 0
    i = 0
    # buffering=0: each write() is one syscall, one st record
    with open(path, "wb", buffering=0) as f:
        while written < total:
            for _ in range(BURST):
                if written >= total:
                    break
                f.write(block_bytes(i))
                i += 1
                written += CHUNK
            time.sleep(PAUSE_S)
    print(f"burst_writer: wrote {written} bytes in {i} records to {path}",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
