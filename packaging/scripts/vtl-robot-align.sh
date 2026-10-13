#!/bin/sh
# Periodic full reconcile kernel↔DB with safety gates (vtl-robot-align.timer, hourly).
# - Evacuates orphan/shelved media from the kernel changer.
# - Pulls kernel inventory into DB (slots/drives.tape_id) ONLY when every drift
#   is two-sided (both DB and kernel know the tape, location differs);
#   single-sided drifts are refused and left for manual review.
# Requires robot_sync=true in vtl.conf. Kernel contact surface is the same
# read-only GET_INVENTORY ioctl the web UI issues on every page view.
set -eu

PREFIX="${VTL_PREFIX:-/opt/vtladm}"
VTL_VAR="${VTL_VAR:-$PREFIX/var}"
VTL_CONF="${VTL_CONF_PATH:-$VTL_VAR/vtl.conf}"

if [ ! -x "$PREFIX/bin/vtladm" ]; then
    echo "vtl-robot-align: vtladm not found at $PREFIX/bin/vtladm" >&2
    exit 1
fi
if [ ! -c /dev/vtl ]; then
    echo "vtl-robot-align: /dev/vtl missing (load vtl.ko?)" >&2
    exit 0
fi
if [ ! -f "$VTL_CONF" ]; then
    echo "vtl-robot-align: no vtl.conf" >&2
    exit 0
fi
if ! grep -qE '^[[:space:]]*robot_sync[[:space:]]*=[[:space:]]*true' "$VTL_CONF" 2>/dev/null; then
    exit 0
fi

echo "=== vtl-robot-align $(date -Iseconds 2>/dev/null || date) ==="
if grep -qE '^[[:space:]]*auto_reconcile_pull[[:space:]]*=[[:space:]]*false' "$VTL_CONF" 2>/dev/null; then
    echo "note: auto_reconcile_pull=false — orphan evacuation only, no kernel→DB pull"
fi
if "$PREFIX/bin/vtladm" robot auto-align-all; then
    echo "OK  auto-align complete"
    exit 0
fi
echo "WARN vtl-robot-align failed (see vtladm log)" >&2
exit 1
