#!/bin/bash
# VTL Block Header Test Suite
# Validates that each SCSI WRITE creates a self-describing block
# preserving logical tape block boundaries.

set -e

TAPE="/dev/nst0"
SG_TAPE="/dev/sg3"
TAPE_FILE="/opt/vtladm/var/tapes/aaa_Tape01.vtltape"
PASS=0
FAIL=0

pass() { echo "  ✓ $1"; PASS=$((PASS+1)); }
fail() { echo "  ✗ $1"; FAIL=$((FAIL+1)); }

echo "=== VTL Block Header Test Suite ==="
echo "Tape: $TAPE, SG: $SG_TAPE"

# ---- Test 1: Single write/read roundtrip ----
echo ""
echo "--- Test 1: Small block roundtrip (100 bytes) ---"
mt -f $TAPE rewind
python3 -c "import os; os.write(os.open('$SG_TAPE', os.O_RDWR), b'A'*100)" 2>/dev/null || true
# Use dd through st driver
echo -n "$(python3 -c 'print("T1_" + "B"*95)')" > /tmp/t1_in.bin
dd if=/tmp/t1_in.bin of=$TAPE bs=100 count=1 2>/dev/null
mt -f $TAPE rewind
dd if=$TAPE of=/tmp/t1_out.bin bs=100 count=1 2>/dev/null
if cmp -s /tmp/t1_in.bin /tmp/t1_out.bin; then
    pass "100-byte write/read exact match"
else
    fail "100-byte write/read mismatch"
    cmp -l /tmp/t1_in.bin /tmp/t1_out.bin | head -5
fi

# ---- Test 2: Multi-block with different sizes ----
echo ""
echo "--- Test 2: Multi-block with varying sizes ---"
mt -f $TAPE rewind
echo -n "BLOCK_A_32bytes_aaaaaaaaaaaaaa" > /tmp/t2_a.bin
echo -n "BLOCK_B_64bytes_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" > /tmp/t2_b.bin
echo -n "BLOCK_C_128b_ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc" > /tmp/t2_c.bin

dd if=/tmp/t2_a.bin of=$TAPE bs=32 count=1 2>/dev/null
dd if=/tmp/t2_b.bin of=$TAPE bs=64 count=1 2>/dev/null
dd if=/tmp/t2_c.bin of=$TAPE bs=128 count=1 2>/dev/null

mt -f $TAPE rewind
dd if=$TAPE of=/tmp/t2_a_out.bin bs=32 count=1 2>/dev/null
dd if=$TAPE of=/tmp/t2_b_out.bin bs=64 count=1 2>/dev/null
dd if=$TAPE of=/tmp/t2_c_out.bin bs=128 count=1 2>/dev/null

MATCH=0
cmp -s /tmp/t2_a.bin /tmp/t2_a_out.bin && MATCH=$((MATCH+1))
cmp -s /tmp/t2_b.bin /tmp/t2_b_out.bin && MATCH=$((MATCH+1))
cmp -s /tmp/t2_c.bin /tmp/t2_c_out.bin && MATCH=$((MATCH+1))
if [ $MATCH -eq 3 ]; then
    pass "3 blocks (32+64+128 bytes) all exact match"
else
    fail "block mismatch ($MATCH/3 matched)"
fi

# ---- Test 3: Block boundaries survive filemarks ----
echo ""
echo "--- Test 3: Filemark between blocks ---"
mt -f $TAPE rewind
echo -n "BEFORE_FM_512b_$(python3 -c 'print("X"*499)')" > /tmp/t3_before.bin
echo -n "AFTER_FM_256b_$(python3 -c 'print("Y"*243)')"  > /tmp/t3_after.bin

dd if=/tmp/t3_before.bin of=$TAPE bs=512 count=1 2>/dev/null
mt -f $TAPE weof 1
dd if=/tmp/t3_after.bin of=$TAPE bs=256 count=1 2>/dev/null

mt -f $TAPE rewind
dd if=$TAPE of=/tmp/t3_before_out.bin bs=512 count=1 2>/dev/null
dd if=$TAPE of=/tmp/t3_after_out.bin bs=256 count=1 2>/dev/null

MATCH=0
cmp -s /tmp/t3_before.bin /tmp/t3_before_out.bin && MATCH=$((MATCH+1))
cmp -s /tmp/t3_after.bin /tmp/t3_after_out.bin && MATCH=$((MATCH+1))
if [ $MATCH -eq 2 ]; then
    pass "Blocks separated by filemark both intact"
else
    fail "Filemark test mismatch ($MATCH/2)"
fi

# ---- Test 4: Large block (32KB = typical st driver chunk) ----
echo ""
echo "--- Test 4: 32KB block (st driver typical size) ---"
mt -f $TAPE rewind
python3 -c "
import os, struct
data = b'LK_32KB_' + struct.pack('<I', 0xDEADBEEF) + bytes(range(256)) * 127
data = data[:32768]
open('/tmp/t4_in.bin', 'wb').write(data)
"
dd if=/tmp/t4_in.bin of=$TAPE bs=32k count=1 2>/dev/null
mt -f $TAPE rewind
dd if=$TAPE of=/tmp/t4_out.bin bs=32k count=1 2>/dev/null
if cmp -s /tmp/t4_in.bin /tmp/t4_out.bin; then
    pass "32KB block roundtrip exact match"
else
    fail "32KB block mismatch"
    cmp -l /tmp/t4_in.bin /tmp/t4_out.bin 2>/dev/null | head -5
fi

# ---- Test 5: Verify VLTB header on disk ----
echo ""
echo "--- Test 5: VLTB header presence on disk ---"
mt -f $TAPE rewind
echo -n "HEADER_TEST_DATA_1234567890" > /tmp/t5_in.bin
dd if=/tmp/t5_in.bin of=$TAPE bs=32 count=1 2>/dev/null
sync
# Read the file directly to check header format
HEADER_MAGIC=$(xxd -l 4 -p $TAPE_FILE)
if [ "$HEADER_MAGIC" = "564c5442" ]; then
    pass "VLTB magic (564c5442) found at tape file start"
else
    fail "VLTB magic missing: got $HEADER_MAGIC"
fi

# Check uncompressed_size field (bytes 4-7, big-endian)
UNCOMP_HEX=$(xxd -s 4 -l 4 -p $TAPE_FILE)
UNCOMP_BE=$(echo $UNCOMP_HEX | sed 's/\(..\)\(..\)\(..\)\(..\)/\1\2\3\4/')
UNCOMP_DEC=$(printf "%d" 0x$UNCOMP_BE)
if [ "$UNCOMP_DEC" = "32" ]; then
    pass "Header uncompressed_size = 32 (matches write size)"
else
    fail "Header uncompressed_size = $UNCOMP_DEC, expected 32"
fi

echo ""
echo "============================================"
echo "Results: $PASS passed, $FAIL failed"
echo "============================================"
exit $FAIL
