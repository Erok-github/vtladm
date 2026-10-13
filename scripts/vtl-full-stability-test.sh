#!/bin/bash
# VTL 全栈稳定性测试 — 覆盖内核 SCSI + vtladm 用户态 + 磁带 I/O
#
# Usage (root):
#   bash vtl-full-stability-test.sh smoke       # 快速冒烟 (~5 min)
#   bash vtl-full-stability-test.sh stress      # 标准压力 (~30 min)
#   bash vtl-full-stability-test.sh soak        # 长期浸泡 (默认 1h，VTL_SOAK_MIN 覆盖)
#   bash vtl-full-stability-test.sh concurrency # 并发压测
#   bash vtl-full-stability-test.sh all         # 按顺序全部执行
#
# 环境变量:
#   VTL_SOAK_MIN       浸泡分钟数 (默认 60，最小 5，最大 1440)
#   VTL_REPORT_DIR     报告目录 (默认 /var/log/vtl-stress)
#   VTL_MTX_DEV        机械手设备 (默认 /dev/sg5)
#   VTL_TAPE_ST_DEV    磁带 st 设备 (默认 /dev/st0)
#   VTL_VTLADM_BIN     vtladm 路径 (默认 /opt/vtladm/bin/vtladm)
#   VTL_WEB_URL        Web API 地址 (默认 http://localhost:8765)

set -euo pipefail

# ─── 配置 ─────────────────────────────────────────────
VTL_SOAK_MIN="${VTL_SOAK_MIN:-60}"
VTL_REPORT_DIR="${VTL_REPORT_DIR:-/var/log/vtl-stress}"
VTL_MTX_DEV="${VTL_MTX_DEV:-/dev/sg5}"
VTL_TAPE_ST_DEV="${VTL_TAPE_ST_DEV:-/dev/st0}"
# SG 设备用于直接 SCSI 命令 (SYNCHRONIZE CACHE 等)，绕过 st 驱动 fsync 限制
_vtl_find_tape_sg() {
    local sg
    sg=$(readlink -f /sys/class/scsi_tape/st0/device/generic 2>/dev/null | xargs basename 2>/dev/null)
    echo "/dev/${sg:-sg6}"
}
VTL_TAPE_SG_DEV="${VTL_TAPE_SG_DEV:-$(_vtl_find_tape_sg)}"
VTL_VTLADM_BIN="${VTL_VTLADM_BIN:-/opt/vtladm/bin/vtladm}"
VTL_WEB_URL="${VTL_WEB_URL:-http://localhost:8765}"

PHASE="${1:-smoke}"
RUN_ID="$(date +%Y%m%d-%H%M%S)"
REPORT="$VTL_REPORT_DIR/run-$RUN_ID"
PASS=0
FAIL=0
SKIP=0

mkdir -p "$REPORT"

# ─── 工具函数 ──────────────────────────────────────────
log()  { echo "[$(date +%H:%M:%S)] $*" | tee -a "$REPORT/run.log"; }
ok()   { PASS=$((PASS + 1)); log "  ✓ $1"; }
bad()  { FAIL=$((FAIL + 1)); log "  ✗ $1"; }
skip() { SKIP=$((SKIP + 1)); log "  - $1 (skip)"; }

die() {
    log "FATAL: $1"
    final_report
    exit 1
}

final_report() {
    local total=$((PASS + FAIL + SKIP))
    echo ""
    echo "══════════════════════════════════════════════"
    echo "  测试完成: $PASS 通过 / $FAIL 失败 / $SKIP 跳过 (共 $total)"
    echo "  报告目录: $REPORT"
    echo "  运行 ID:  $RUN_ID"
    echo "══════════════════════════════════════════════"
}

snapshot_sys() {
    local tag="$1"
    {
        echo "=== $tag @ $(date -Iseconds) ==="
        echo "--- uptime ---"
        uptime
        echo "--- memory ---"
        free -h
        echo "--- vtl.ko refcnt ---"
        cat /sys/module/vtl/refcnt 2>/dev/null || echo "N/A"
        echo "--- /proc/slabinfo(vtl) ---"
        grep -i vtl /proc/slabinfo 2>/dev/null || echo "none"
        echo "--- lsscsi vtl ---"
        lsscsi -g 2>/dev/null | grep vtl || echo "none"
    } >> "$REPORT/snapshots.log"
}

check_kernel_errors() {
    local tag="$1"
    local new_errors
    # Only capture real errors. Exclude:
    #   - module verification failed (unsigned out-of-tree module, expected)
    #   - systemd-sysv noise, PostgreSQL noise
    #   - "scsi.*error" matches are too broad; rely on oops/panic/bug/vtl-specific patterns
    new_errors=$(dmesg | grep -iE "oops|panic|bug:|vtl.*(error|fail|warn|corrupt|leak|deadlock|oom|timed.out)" \
        | grep -v "module verification failed\|tainting kernel" \
        | grep -v "systemd-sysv\|MBAMaster\|PostgreSQL\|MBAData" || true)
    if [ -n "$new_errors" ]; then
        echo "=== kernel errors @ $tag ===" >> "$REPORT/kernel_errors.log"
        echo "$new_errors" >> "$REPORT/kernel_errors.log"
    fi
}

# ─── 前置检查 ──────────────────────────────────────────
preflight() {
    log "=== 前置检查 ==="

    # vtl.ko 已加载
    if lsmod | grep -q "^vtl "; then
        ok "vtl.ko 已加载"
    else
        die "vtl.ko 未加载，请先 insmod"
    fi

    # lsscsi 能发现 VTL 设备
    if lsscsi -g 2>/dev/null | grep -q "IBM.*03584L32\|IBM.*ULT3580"; then
        ok "lsscsi 发现 VTL SCSI 设备"
    else
        bad "lsscsi 未发现 VTL 设备"
    fi

    # mtx 可用
    if mtx -f "$VTL_MTX_DEV" status &>/dev/null; then
        ok "mtx status 正常"
    else
        die "mtx status 失败"
    fi

    # vtladm 可用
    if "$VTL_VTLADM_BIN" status &>/dev/null; then
        ok "vtladm CLI 正常"
    else
        bad "vtladm CLI 失败"
    fi

    # vtladm serve 运行
    if curl -s -o /dev/null -w "%{http_code}" "$VTL_WEB_URL/api/v1/status" 2>/dev/null | grep -q "200\|302"; then
        ok "vtladm Web 可连通"
    else
        skip "vtladm Web 不可连通或需登录"
    fi

    # 检查当前磁带状态
    log "当前磁带状态:"
    mtx -f "$VTL_MTX_DEV" status 2>/dev/null | tee -a "$REPORT/run.log"

    # 检查托盘占用
    local holders
    holders=$(fuser /dev/st0 /dev/sg5 /dev/sg6 2>/dev/null || true)
    if [ -n "$holders" ]; then
        log "以下进程持有 VTL 设备: $holders"
    fi

    snapshot_sys "preflight"
    check_kernel_errors "preflight"
}

# ─── SCSI 压力测试 ──────────────────────────────────────
scsi_stress() {
    local duration_sec="${1:-300}"   # 默认 5 分钟
    local deadline
    deadline=$(($(date +%s) + duration_sec))

    log "=== SCSI 透传压力测试 (${duration_sec}s) ==="

    local n=0
    while [ $(date +%s) -lt $deadline ]; do
        n=$((n + 1))

        # TUR on changer + both tape sg devices
        {
            echo "TUR loop $n @ $(date +%H:%M:%S)"
            /tmp/vtl_tur /dev/sg5 /dev/sg6 /dev/sg7 2>&1 || true
        } >> "$REPORT/scsi_tur.log" 2>&1

        # mtx status
        mtx -f "$VTL_MTX_DEV" status >> "$REPORT/scsi_mtx.log" 2>&1 || {
            bad "mtx status 失败 (loop $n)"
        }

        # sg_inq on tape
        sg_inq /dev/sg6 >> "$REPORT/scsi_inq.log" 2>&1 || {
            bad "sg_inq /dev/sg6 失败 (loop $n)"
        }

        # Check kernel errors every 50 loops (avoid log noise)
        if [ $((n % 50)) -eq 0 ]; then
            check_kernel_errors "scsi_loop_$n"
        fi
    done

    ok "SCSI 压力测试完成 ($n 轮)"
}

# ─── 磁带 Load/Unload 循环 ──────────────────────────────
tape_cycle() {
    local cycles="${1:-10}"

    log "=== 磁带 Load/Unload 循环 ($cycles 轮) ==="

    # 获取当前状态，找到加载的磁带和空槽位
    local drive0_loaded slot_num unload_target

    for i in $(seq 1 $cycles); do
        log "--- 第 $i 轮 ---"

        # 获取 loaded tape 信息
        drive0_loaded=$(mtx -f "$VTL_MTX_DEV" status 2>/dev/null | grep "Data Transfer Element 0:" || true)

        if echo "$drive0_loaded" | grep -q "Full"; then
            # Drive 0 有磁带 → 卸载到空槽位
            unload_target=$(mtx -f "$VTL_MTX_DEV" status 2>/dev/null | grep "Storage Element" | grep "Empty" | head -1 | grep -oP 'Storage Element \K\d+')
            if [ -n "$unload_target" ]; then
                log "  卸载 drive0 → slot $unload_target"
                mtx -f "$VTL_MTX_DEV" unload "$unload_target" 0 >> "$REPORT/tape_cycle.log" 2>&1 || {
                    bad "unload 失败 (loop $i)"
                    continue
                }
            else
                log "  无空槽位，跳过 unload"
                skip "无空槽位可卸载"
                continue
            fi
        else
            # Drive 0 为空 → 从满槽加载
            slot_num=$(mtx -f "$VTL_MTX_DEV" status 2>/dev/null | grep "Storage Element.*Full" | head -1 | grep -oP 'Storage Element \K\d+')
            if [ -n "$slot_num" ]; then
                log "  加载 slot $slot_num → drive 0"
                mtx -f "$VTL_MTX_DEV" load "$slot_num" 0 >> "$REPORT/tape_cycle.log" 2>&1 || {
                    bad "load 失败 (loop $i)"
                    continue
                }
            else
                log "  无满槽磁带，跳过 load"
                skip "无磁带可加载"
                continue
            fi
        fi

        ok "  第 $i 轮完成"
        check_kernel_errors "tape_cycle_$i"
    done
}

# ─── 磁带读写测试 ────────────────────────────────────────
tape_io_test() {
    local iter="${1:-5}"

    log "=== 磁带读写测试 ($iter 轮) ==="

    # 确保有磁带在 drive 0
    local loaded
    loaded=$(mtx -f "$VTL_MTX_DEV" status 2>/dev/null | grep "Data Transfer Element 0:" || true)
    if ! echo "$loaded" | grep -q "Full"; then
        log "  先将磁带加载到 drive 0..."
        local first_full
        first_full=$(mtx -f "$VTL_MTX_DEV" status 2>/dev/null | grep "Storage Element.*Full" | head -1 | grep -oP 'Storage Element \K\d+')
        if [ -n "$first_full" ]; then
            mtx -f "$VTL_MTX_DEV" load "$first_full" 0 >> "$REPORT/tape_io.log" 2>&1 || {
                skip "无法加载磁带，跳过 I/O 测试"
                return
            }
        else
            skip "无可用磁带，跳过 I/O 测试"
            return
        fi
    fi

    for i in $(seq 1 $iter); do
        log "--- I/O 第 $i 轮 ---"

        # 写入测试数据（先写数据，再单独 fsync）
        if dd if=/dev/urandom of="$VTL_TAPE_ST_DEV" bs=32k count=2 conv=notrunc 2>>"$REPORT/tape_io.log"; then
            ok "  写入 64KB 成功"
        else
            bad "  写入失败"
        fi

        # 显式 SYNCHRONIZE CACHE (sg_sync 直接发 SCSI 命令，绕过 st 驱动 fsync 限制)
        if sg_sync "$VTL_TAPE_SG_DEV" 2>>"$REPORT/tape_io.log"; then
            ok "  SYNCHRONIZE CACHE 成功"
        else
            bad "  SYNCHRONIZE CACHE 失败"
        fi

        # 回绕
        if mt -f "$VTL_TAPE_ST_DEV" rewind 2>>"$REPORT/tape_io.log"; then
            ok "  回绕成功"
        else
            bad "  回绕失败"
        fi

        # 读取校验 (读回已写的数据量)
        local read_bytes
        read_bytes=$(dd if="$VTL_TAPE_ST_DEV" bs=32k count=2 2>&1 | wc -c)
        if [ "$read_bytes" -ge 65536 ]; then
            ok "  读取 ${read_bytes} 字节"
        else
            bad "  读取不足 (got $read_bytes)"
        fi

        mt -f "$VTL_TAPE_ST_DEV" rewind 2>/dev/null || true
        check_kernel_errors "tape_io_$i"
    done
}

# ─── vtladm CLI 压力测试 ─────────────────────────────────
vtladm_cli_stress() {
    local duration_sec="${1:-300}"

    log "=== vtladm CLI 压力测试 (${duration_sec}s) ==="
    local deadline
    deadline=$(($(date +%s) + duration_sec))
    local n=0

    while [ $(date +%s) -lt $deadline ]; do
        n=$((n + 1))

        "$VTL_VTLADM_BIN" inventory >> "$REPORT/vtladm_inv.log" 2>&1 || true
        "$VTL_VTLADM_BIN" status   >> "$REPORT/vtladm_status.log" 2>&1 || true
        "$VTL_VTLADM_BIN" shelf    >> "$REPORT/vtladm_shelf.log" 2>&1 || true

        # patrol 每 5 轮跑一次（避免太重）
        if [ $((n % 5)) -eq 0 ]; then
            "$VTL_VTLADM_BIN" patrol >> "$REPORT/vtladm_patrol.log" 2>&1 || true
        fi
    done

    ok "vtladm CLI 压力完成 ($n 轮)"
}

# ─── 长时间浸泡 ──────────────────────────────────────────
soak() {
    local duration_min="${1:-$VTL_SOAK_MIN}"
    local duration_sec=$((duration_min * 60))
    local deadline snapshot_interval
    deadline=$(($(date +%s) + duration_sec))
    snapshot_interval=300  # 每 5 分钟采集快照

    log "=== 浸泡测试 (${duration_min} 分钟) ==="
    log "开始时间: $(date)"

    while [ $(date +%s) -lt $deadline ]; do
        local remaining=$(( (deadline - $(date +%s)) / 60 ))
        log "浸泡中... 剩余 ${remaining} 分钟"

        # 周期性 snapshot
        snapshot_sys "soak_$(date +%H%M)"

        # 轻量 SCSI 探测
        mtx -f "$VTL_MTX_DEV" status &>/dev/null || bad "mtx 状态检查失败"
        sg_inq /dev/sg6 &>/dev/null || bad "sg_inq 失败"
        "$VTL_VTLADM_BIN" inventory &>/dev/null || bad "inventory 失败"

        # 轮询 sleep
        local sleep_time=$((snapshot_interval < remaining * 60 ? snapshot_interval : remaining * 60))
        sleep "$sleep_time"
    done

    ok "浸泡测试完成 (${duration_min} 分钟)"
    snapshot_sys "soak_final"
}

# ─── 并发压测 ────────────────────────────────────────────
concurrency_stress() {
    local duration_sec="${1:-120}"

    log "=== 并发压测 (${duration_sec}s) ==="

    # 并发：mtx status + sg_inq + vtladm inventory + TUR 同时运行
    (
        for i in $(seq 1 100); do
            mtx -f "$VTL_MTX_DEV" status &>/dev/null || true
            sleep 0.2
        done
    ) &
    local pid1=$!

    (
        for i in $(seq 1 100); do
            sg_inq /dev/sg6 &>/dev/null || true
            sleep 0.2
        done
    ) &
    local pid2=$!

    (
        for i in $(seq 1 50); do
            "$VTL_VTLADM_BIN" inventory &>/dev/null || true
            sleep 0.5
        done
    ) &
    local pid3=$!

    (
        for i in $(seq 1 100); do
            /tmp/vtl_tur /dev/sg5 /dev/sg6 /dev/sg7 &>/dev/null || true
            sleep 0.2
        done
    ) &
    local pid4=$!

    # 等待所有后台任务
    wait $pid1 2>/dev/null || true
    wait $pid2 2>/dev/null || true
    wait $pid3 2>/dev/null || true
    wait $pid4 2>/dev/null || true

    ok "并发压测完成"
    check_kernel_errors "concurrency"
}

# ─── 内存泄漏检测 ────────────────────────────────────────
memory_check() {
    log "=== 内存检测 ==="

    # vtl.ko 模块内存
    local mod_mem
    mod_mem=$(awk '/^vtl /{print $2}' /proc/modules 2>/dev/null || echo "0")
    log "  vtl.ko 占用: ${mod_mem:-0} 字节"

    # vtladm 进程内存
    local pid rss
    pid=$(pgrep -f "vtladm serve" | head -1)
    if [ -n "$pid" ]; then
        rss=$(awk '/VmRSS/{print $2}' "/proc/$pid/status" 2>/dev/null || echo "0")
        log "  vtladm serve (PID $pid): RSS=${rss:-0} kB"
    fi

    # slab 缓存
    if grep -q vtl /proc/slabinfo 2>/dev/null; then
        log "  slab 缓存:"
        grep -E "^#|vtl" /proc/slabinfo 2>/dev/null | tee -a "$REPORT/run.log"
    fi

    # 比较初始 vs 当前
    if [ -f "$REPORT/initial_memory.txt" ]; then
        log "  内存变化对比:"
        diff "$REPORT/initial_memory.txt" <(free -h) || true
    fi
}

# ─── 报告生成 ────────────────────────────────────────────
generate_report() {
    log "=== 生成报告 ==="

    {
        echo "# VTL 稳定性测试报告"
        echo ""
        echo "- 运行 ID: $RUN_ID"
        echo "- 阶段: $PHASE"
        echo "- 时间: $(date -Iseconds)"
        echo "- 结果: $PASS 通过 / $FAIL 失败 / $SKIP 跳过"
        echo ""
        echo "## 系统信息"
        echo '```'
        echo "内核: $(uname -r)"
        echo "CPU:  $(nproc) cores"
        free -h
        echo '```'
        echo ""
        echo "## 内核日志错误"
        if [ -f "$REPORT/kernel_errors.log" ] && [ -s "$REPORT/kernel_errors.log" ]; then
            echo '```'
            # Dedup: sort by section, keep first occurrence of each unique error line
            awk '
              /^=== kernel errors/ { tag=$0; next }
              { if (!seen[$0]++) { if (tag && tag!=last_tag) { print ""; print tag; last_tag=tag }; print } }
            ' "$REPORT/kernel_errors.log"
            echo '```'
        else
            echo "无"
        fi
        echo ""
        echo "## 系统快照"
        echo '```'
        tail -100 "$REPORT/snapshots.log" 2>/dev/null || echo "无快照"
        echo '```'
    } > "$REPORT/report.md"

    ok "报告已生成: $REPORT/report.md"
}

# ─── 清理 ────────────────────────────────────────────────
cleanup() {
    log "=== 清理 ==="
    # 确保 prod_vtl_tur 存在
    if [ ! -x /tmp/vtl_tur ]; then
        log "  编译 vtl_tur..."
        gcc -O2 -o /tmp/vtl_tur /tmp/test_all_sg.c 2>/dev/null || true
    fi
    log "清理完成"
}

# ─── 主流程 ──────────────────────────────────────────────
main() {
    echo ""
    echo "██╗   ██╗████████╗██╗      ███████╗████████╗██████╗ ███████╗███████╗███████╗"
    echo "██║   ██║╚══██╔══╝██║      ██╔════╝╚══██╔══╝██╔══██╗██╔════╝██╔════╝██╔════╝"
    echo "██║   ██║   ██║   ██║█████╗███████╗   ██║   ██████╔╝█████╗  ███████╗███████╗"
    echo "╚██╗ ██╔╝   ██║   ██║╚════╝╚════██║   ██║   ██╔══██╗██╔══╝  ╚════██║╚════██║"
    echo " ╚████╔╝    ██║   ███████╗ ███████║   ██║   ██║  ██║███████╗███████║███████║"
    echo "  ╚═══╝     ╚═╝   ╚══════╝ ╚══════╝   ╚═╝   ╚═╝  ╚═╝╚══════╝╚══════╝╚══════╝"
    echo ""
    echo "  VTL 全栈稳定性测试"
    echo "  报告: $REPORT"
    echo ""

    cleanup
    preflight
    snapshot_sys "start"
    free -h > "$REPORT/initial_memory.txt"

    case "$PHASE" in
        smoke)
            log ">>> 冒烟测试 <<<"
            scsi_stress 60           # 1 分钟 SCSI 压力
            tape_cycle 3             # 3 轮 load/unload
            tape_io_test 2           # 2 轮读写
            vtladm_cli_stress 60     # 1 分钟 CLI 压力
            memory_check
            ;;

        stress)
            log ">>> 标准压力测试 <<<"
            scsi_stress 300          # 5 分钟 SCSI 压力
            tape_cycle 10            # 10 轮 load/unload
            tape_io_test 5           # 5 轮读写
            vtladm_cli_stress 300    # 5 分钟 CLI
            concurrency_stress 120   # 2 分钟并发
            memory_check
            ;;

        soak)
            log ">>> 浸泡测试 <<<"
            scsi_stress 120          # 2 分钟 SCSI 预热
            tape_cycle 3             # 3 轮 load/unload
            soak "$VTL_SOAK_MIN"     # 长时间浸泡
            memory_check
            ;;

        concurrency)
            log ">>> 并发压测 <<<"
            concurrency_stress 300   # 5 分钟并发
            memory_check
            ;;

        all)
            log ">>> 全流程测试 <<<"

            log "--- Phase 1: 冒烟 ---"
            scsi_stress 60
            tape_cycle 3
            tape_io_test 2
            vtladm_cli_stress 60
            memory_check

            log "--- Phase 2: 并发 ---"
            concurrency_stress 120

            log "--- Phase 3: 浸泡 ---"
            local soak_min=$((VTL_SOAK_MIN / 3))
            [ "$soak_min" -lt 5 ] && soak_min=5
            scsi_stress 120
            tape_cycle 5
            tape_io_test 3
            soak "$soak_min"

            log "--- Phase 4: 恢复测试 ---"
            concurrency_stress 60
            scsi_stress 60
            vtladm_cli_stress 60
            memory_check
            ;;

        preflight-only)
            log ">>> 仅前置检查 <<<"
            ;;

        *)
            echo "Usage: $0 {smoke|stress|soak|concurrency|all|preflight-only}"
            echo ""
            echo "  smoke         快速冒烟测试 (~5 min)"
            echo "  stress        标准压力测试 (~30 min)"
            echo "  soak          长期浸泡 (默认 ${VTL_SOAK_MIN}min，VTL_SOAK_MIN 覆盖)"
            echo "  concurrency   并发压测"
            echo "  all           全流程测试"
            echo "  preflight-only  仅前置检查"
            exit 1
            ;;
    esac

    generate_report
    final_report
}

main "$@"
