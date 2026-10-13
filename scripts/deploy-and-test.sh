#!/bin/bash
# deploy-and-test.sh — 自动部署 vtladm 到远程服务器并运行验证测试
# Usage: ./scripts/deploy-and-test.sh [user@]host [--quick]
#   --quick  跳过编译，只测试 (假设 vtl.ko 已是最新)

set -eu

TARGET="${1:-root@192.168.5.63}"
QUICK="${2:-}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PASS="${VTL_DEPLOY_PASS:-4rfVBNji9}"

_ssh()  { sshpass -p "$PASS" ssh -o StrictHostKeyChecking=no "$TARGET" "$@"; }
_scp()  { sshpass -p "$PASS" scp -o StrictHostKeyChecking=no "$@"; }

echo "=== vtladm 自动部署到 $TARGET ==="

# ---------- 1. 同步源码 ----------
echo ">> 同步源码..."
_scp "$ROOT/kernel/src/vtl_main.c"   "$TARGET:/root/vtladm/kernel/src/"
_scp "$ROOT/kernel/src/vtl_scsi.c"   "$TARGET:/root/vtladm/kernel/src/"
_scp "$ROOT/kernel/src/vtl_tape.c"   "$TARGET:/root/vtladm/kernel/src/"
_scp "$ROOT/kernel/include/vtl.h"    "$TARGET:/root/vtladm/kernel/include/"
_scp "$ROOT/install.sh"              "$TARGET:/root/vtladm/"

# ---------- 2. 编译 ----------
if [ "$QUICK" != "--quick" ]; then
  echo ">> 远程编译..."
  _ssh "
    cd /root/vtladm/kernel
    make clean 2>/dev/null
    make 2>&1
    echo BUILD=\$?
  "
fi

# ---------- 3. 部署 st 配置 ----------
echo ">> 部署 st 配置..."
_ssh "
  cat > /etc/modprobe.d/vtl-st.conf <<'STEOF'
options st try_direct_io=1 try_rdio=1 try_wdio=1 buffer_kbs=32
STEOF
  echo 'vtl-st.conf written'
"

# ---------- 4. 重载模块 ----------
echo ">> 重载模块..."
_ssh "
  systemctl stop vtladm-web vtl-kernel 2>/dev/null || true
  rmmod vtl 2>/dev/null || true
  rmmod st  2>/dev/null || true
  sleep 1
  modprobe st 2>&1
  sleep 1
  cp /root/vtladm/kernel/vtl.ko /opt/vtladm/ko/vtl.ko
  insmod /opt/vtladm/ko/vtl.ko 2>&1
  sleep 8
  # 恢复设备
  for _sd in /sys/class/scsi_device/*/device; do
    _t=\$(cat \"\$_sd/type\" 2>/dev/null) || continue
    echo running > \"\$_sd/state\" 2>/dev/null || true
  done
"

# ---------- 5. 验证 ----------
echo ""
echo "=== 验证测试 ==="

echo "--- direct I/O ---"
_ssh "dmesg | grep 'try.direct' | tail -3"

echo "--- 设备 ---"
_ssh "lsscsi -g 2>/dev/null | grep VTL; cat /sys/class/scsi_device/*/device/state"

echo "--- mt rewind ---"
_ssh "
  mtx -f /dev/sg2 load 1 0 2>&1
  sleep 1
  echo '1st:' ; mt -f /dev/st0 rewind 2>&1
  echo '2nd:' ; mt -f /dev/st0 rewind 2>&1
"

echo "--- dd write/read 真实文件 ---"
_ssh "
  _F=/var/log/messages-20260611
  [ -f \$_F ] || _F=/var/log/messages
  _SZ=\$(stat -c%s \$_F)
  _BLKS=\$(( (_SZ + 32767) / 32768 ))
  echo \"文件: \$_F (\$_SZ bytes, \$_BLKS x 32KB blocks)\"

  dd if=\$_F of=/dev/nst0 bs=32k 2>&1
  mt -f /dev/st0 rewind 2>&1
  dd if=/dev/nst0 bs=32k count=\$_BLKS of=/tmp/_vttest 2>&1

  _RD=\$(stat -c%s /tmp/_vttest 2>/dev/null || echo 0)
  echo \"写入: \$_SZ, 读回: \$_RD, padding: \$((_RD - _SZ)) bytes\"
  # Verify content up to original size matches
  if cmp -n \$_SZ \$_F /tmp/_vttest 2>&1; then
    echo '✓ 数据内容完全匹配! (末尾 \$((_RD - _SZ)) bytes 零填充)'
  else
    echo '✗ 数据内容不匹配'
  fi
  rm -f /tmp/_vttest
"

echo ""
echo "=== 部署完成 ==="
