#!/bin/bash
# vtl_realdata_test.sh - VTL 真实数据读写测试
# 数据源: 系统日志，恢复到: /root/claude-code/，含 SHA256 数据比对
# 默认强制落盘 (sync + drop_caches)，--cache 允许使用页缓存
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

CHANGER="/dev/sg5"
RESTORE_DIR="/root/claude-code/vtl_restore_test"
ARCHIVE_FILE="/tmp/vtl_test_syslogs.tar"
RESTORED_FILE="/tmp/vtl_test_syslogs_restored.tar"
REPORT_FILE="/tmp/vtl_realdata_report_$(date +%Y%m%d_%H%M%S).txt"
TAPE_DRIVE=""
TAPE_DRIVE_IDX=""
TAPE_SLOT=""
TAPE_VOLTAG=""

# 默认强制落盘; --cache 关闭
FORCE_SYNC=true

log_info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
log_ok()   { echo -e "${GREEN}[PASS]${NC} $*"; }
log_fail() { echo -e "${RED}[FAIL]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }

report() { echo "$*" >> "$REPORT_FILE"; }

usage() {
    echo "用法: $0 [--cache] [--help]"
    echo "  (无参数)  强制落盘模式: 写入后 sync + drop_caches, 确保从磁盘读回"
    echo "  --cache   缓存模式:   不执行 sync / drop_caches (走页缓存)"
    echo "  --help    显示帮助"
    exit 0
}

banner() {
    local mode
    $FORCE_SYNC && mode="强制落盘 (sync + drop_caches)" || mode="缓存模式"
    echo -e "${CYAN}"
    echo "╔══════════════════════════════════════════════╗"
    echo "║   VTL 真实数据读写测试 - 系统日志数据源      ║"
    printf "║   模式: %-36s ║\n" "$mode"
    echo "╚══════════════════════════════════════════════╝"
    echo -e "${NC}"
}

check_deps() {
    log_info "检查依赖工具..."
    for cmd in mtx mt dd tar sha256sum bc stat; do
        if ! command -v $cmd &>/dev/null; then
            log_fail "缺少命令: $cmd"
            exit 1
        fi
    done
    log_ok "依赖检查通过"
}

check_devices() {
    log_info "检查 VTL 设备..."
    if [ ! -e "$CHANGER" ]; then
        log_fail "机械手设备不存在: $CHANGER"
        exit 1
    fi
    log_ok "机械手: $CHANGER"

    echo ""
    mtx -f "$CHANGER" status 2>/dev/null
    echo ""

    # 找空闲驱动器
    if mtx -f "$CHANGER" status 2>/dev/null | grep -q "Data Transfer Element 0:Empty"; then
        TAPE_DRIVE_IDX=0
        TAPE_DRIVE="/dev/st0"
        log_info "选择驱动器 0 ($TAPE_DRIVE)"
    elif mtx -f "$CHANGER" status 2>/dev/null | grep -q "Data Transfer Element 1:Empty"; then
        TAPE_DRIVE_IDX=1
        TAPE_DRIVE="/dev/st1"
        log_info "选择驱动器 1 ($TAPE_DRIVE)"
    else
        log_fail "没有空闲驱动器"
        exit 1
    fi

    TAPE_SLOT=$(mtx -f "$CHANGER" status 2>/dev/null | grep -E "Storage Element [0-9]+:.*Full" | head -1 | grep -oP 'Storage Element \K[0-9]+')
    if [ -z "$TAPE_SLOT" ]; then
        log_fail "没有可用磁带"
        exit 1
    fi
    TAPE_VOLTAG=$(mtx -f "$CHANGER" status 2>/dev/null | grep -E "Storage Element ${TAPE_SLOT}:.*Full" | grep -oP 'VolumeTag=\K\S+')
    log_info "选择槽位 $TAPE_SLOT (标签: $TAPE_VOLTAG)"

    report "测试时间: $(date)"
    report "强制落盘: $FORCE_SYNC"
    report "驱动器: $TAPE_DRIVE (索引 $TAPE_DRIVE_IDX)"
    report "磁带: 槽位 $TAPE_SLOT, 标签 $TAPE_VOLTAG"
}

load_tape() {
    log_info "加载磁带: 槽位 $TAPE_SLOT -> 驱动器 $TAPE_DRIVE_IDX"
    mtx -f "$CHANGER" load "$TAPE_SLOT" "$TAPE_DRIVE_IDX"
    sleep 3

    for i in $(seq 1 15); do
        if mt -f "$TAPE_DRIVE" status 2>&1 | grep -q "ONLINE"; then
            log_ok "驱动器就绪"
            mt -f "$TAPE_DRIVE" status 2>&1 | head -5
            return 0
        fi
        sleep 1
    done
    log_warn "驱动器可能未完全就绪，继续..."
}

package_logs() {
    log_info "打包系统日志..."

    local log_sources=()
    for f in /var/log/messages /var/log/dmesg /var/log/boot.log /var/log/dnf.log /var/log/vtladm.log; do
        [ -f "$f" ] && log_sources+=("$f")
    done

    if command -v journalctl &>/dev/null; then
        journalctl --no-pager -n 5000 2>/dev/null > /tmp/vtl_test_journalctl.log || true
        [ -s /tmp/vtl_test_journalctl.log ] && log_sources+=("/tmp/vtl_test_journalctl.log")
    fi

    if [ ${#log_sources[@]} -eq 0 ]; then
        log_fail "没有找到系统日志文件"
        exit 1
    fi

    log_info "数据源文件:"
    local total_size=0
    for f in "${log_sources[@]}"; do
        local sz=$(stat -c%s "$f" 2>/dev/null || echo 0)
        total_size=$((total_size + sz))
        echo "  $f ($(numfmt --to=iec $sz 2>/dev/null || echo ${sz}B))"
    done
    log_info "总数据大小: $(numfmt --to=iec $total_size 2>/dev/null || echo ${total_size}B)"

    # 落盘归档文件
    tar cf "$ARCHIVE_FILE" "${log_sources[@]}" 2>/dev/null
    sync "$ARCHIVE_FILE" 2>/dev/null || sync
    local archive_sz=$(stat -c%s "$ARCHIVE_FILE")
    log_ok "归档完成: $ARCHIVE_FILE ($(numfmt --to=iec $archive_sz 2>/dev/null || echo ${archive_sz}B))"

    local sha=$(sha256sum "$ARCHIVE_FILE" | awk '{print $1}')
    log_info "原始数据 SHA256: $sha"
    report "原始归档: $ARCHIVE_FILE"
    report "原始 SHA256: $sha"
    report "归档大小: $archive_sz 字节"
    echo "$sha" > /tmp/vtl_test_orig_sha.txt
}

# 强制将系统所有脏页写回磁盘
flush_all_to_disk() {
    log_info "执行全局 sync 强制落盘..."
    local start=$(date +%s.%N)
    sync
    local elapsed=$(echo "$(date +%s.%N) - $start" | bc)
    log_ok "sync 完成 (耗时 ${elapsed}s)"
    report "sync 落盘耗时: ${elapsed}s"
}

# 清除内核页缓存，确保后续读取从磁盘而非缓存获取
drop_caches() {
    if [ ! -w /proc/sys/vm/drop_caches ]; then
        log_warn "无权限写 /proc/sys/vm/drop_caches (需要 root)，跳过清除缓存"
        report "drop_caches: 跳过 (权限不足)"
        return 1
    fi

    log_info "清除内核页缓存 (drop_caches)..."
    local before_free=$(awk '/^MemFree|^Cached|^Buffers/ {print $1, $2}' /proc/meminfo | tr '\n' ' ')
    echo 3 > /proc/sys/vm/drop_caches
    sleep 0.5
    local after_free=$(awk '/^MemFree|^Cached|^Buffers/ {print $1, $2}' /proc/meminfo | tr '\n' ' ')
    log_ok "缓存已清除"
    log_info "清除前 MemFree/Cached/Buffers: $before_free"
    log_info "清除后 MemFree/Cached/Buffers: $after_free"
    report "drop_caches: 已执行"
    report "清除前: $before_free"
    report "清除后: $after_free"
    return 0
}

write_to_tape() {
    log_info "写入数据到磁带 (${FORCE_SYNC:+强制落盘模式}${FORCE_SYNC:-缓存模式})..."

    mt -f "$TAPE_DRIVE" rewind
    sleep 1
    mt -f "$TAPE_DRIVE" setblk 0 2>/dev/null || true

    local archive_sz=$(stat -c%s "$ARCHIVE_FILE")
    local start_time=$(date +%s.%N)

    # dd 写入磁带 (字符设备不支持 conv=fsync, 落盘由后续 sync 保证)
    if dd if="$ARCHIVE_FILE" of="$TAPE_DRIVE" bs=256k 2>&1; then
        local end_time=$(date +%s.%N)
        local elapsed=$(echo "$end_time - $start_time" | bc)
        local speed_mb=$(echo "scale=2; $archive_sz / 1048576 / $elapsed" | bc 2>/dev/null || echo "N/A")

        log_ok "写入完成: ${elapsed}秒, ${speed_mb} MB/s"
        report "写入耗时: ${elapsed}秒"
        report "写入速度: ${speed_mb} MB/s"
    else
        log_fail "写入磁带失败"
        exit 1
    fi

    # 写两个 EOF 标记 (触发驱动器 flush)
    mt -f "$TAPE_DRIVE" weof 2>/dev/null || true

    # 强制落盘: 全局 sync
    if $FORCE_SYNC; then
        flush_all_to_disk
    fi
}

read_from_tape() {
    log_info "从磁带回读数据 (${FORCE_SYNC:+强制落盘模式}${FORCE_SYNC:-缓存模式})..."

    # 强制落盘: 回读前清除页缓存
    if $FORCE_SYNC; then
        drop_caches || true
    fi

    mt -f "$TAPE_DRIVE" rewind
    sleep 1
    mt -f "$TAPE_DRIVE" setblk 0 2>/dev/null || true

    local archive_sz=$(stat -c%s "$ARCHIVE_FILE")
    local start_time=$(date +%s.%N)

    # dd 读取; iflag=direct 在磁带字符设备上通常不支持, 但缓存已清, 数据须从 VTL 后端文件读取
    if dd if="$TAPE_DRIVE" of="$RESTORED_FILE" bs=256k count=$(( (archive_sz + 262143) / 262144 )) 2>&1; then
        # 确保恢复文件也落盘
        $FORCE_SYNC && sync "$RESTORED_FILE" 2>/dev/null || true

        local end_time=$(date +%s.%N)
        local elapsed=$(echo "$end_time - $start_time" | bc)
        local restored_sz=$(stat -c%s "$RESTORED_FILE")
        local speed_mb=$(echo "scale=2; $restored_sz / 1048576 / $elapsed" | bc 2>/dev/null || echo "N/A")

        log_ok "回读完成: ${elapsed}秒, ${speed_mb} MB/s, 大小=$(numfmt --to=iec $restored_sz 2>/dev/null || echo ${restored_sz}B)"
        report "回读耗时: ${elapsed}秒"
        report "回读速度: ${speed_mb} MB/s"
        report "回读大小: $restored_sz 字节"

        if [ "$restored_sz" -gt "$archive_sz" ]; then
            truncate -s "$archive_sz" "$RESTORED_FILE"
            log_info "已截断到原始大小 $archive_sz 字节"
        fi
    else
        log_fail "从磁带读取失败"
        exit 1
    fi
}

verify_checksum() {
    log_info "=== 数据完整性校验 ==="

    local orig_sha=$(cat /tmp/vtl_test_orig_sha.txt)
    local restored_sha=$(sha256sum "$RESTORED_FILE" | awk '{print $1}')

    log_info "原始 SHA256:   $orig_sha"
    log_info "恢复 SHA256:   $restored_sha"

    if [ "$orig_sha" = "$restored_sha" ]; then
        log_ok "SHA256 校验通过! 数据完整一致"
        report "校验结果: 通过 (SHA256 一致)"
        return 0
    else
        log_fail "SHA256 校验失败! 数据不一致"
        report "校验结果: 失败 (SHA256 不一致)"

        local orig_sz=$(stat -c%s "$ARCHIVE_FILE")
        local restored_sz=$(stat -c%s "$RESTORED_FILE")
        log_info "原始大小: $orig_sz 字节, 恢复大小: $restored_sz 字节"

        if [ "$orig_sz" != "$restored_sz" ]; then
            log_warn "文件大小不一致，仅比较前 $(( orig_sz < restored_sz ? orig_sz : restored_sz )) 字节"
        fi

        local diff_pos=$(cmp -l "$ARCHIVE_FILE" "$RESTORED_FILE" 2>/dev/null | head -1 | awk '{print $1}')
        if [ -n "$diff_pos" ]; then
            log_info "首个差异位置: 字节 $diff_pos"
        fi

        return 1
    fi
}

restore_to_disk() {
    log_info "恢复到目标目录: $RESTORE_DIR"

    rm -rf "$RESTORE_DIR"
    mkdir -p "$RESTORE_DIR"

    tar xf "$RESTORED_FILE" -C "$RESTORE_DIR" 2>/dev/null
    $FORCE_SYNC && sync

    log_ok "恢复完成: $RESTORE_DIR"

    echo ""
    log_info "恢复的文件列表:"
    find "$RESTORE_DIR" -type f -exec ls -lh {} \; 2>/dev/null | head -20
    local file_count=$(find "$RESTORE_DIR" -type f | wc -l)
    log_info "共恢复 $file_count 个文件"

    report "恢复目录: $RESTORE_DIR"
    report "恢复文件数: $file_count"
}

file_level_compare() {
    log_info "=== 文件级别比对 ==="

    # 从归档中提取文件与恢复目录比对 (避免实时日志干扰)
    local tmpdir="/tmp/vtl_test_orig_extract_$$"
    mkdir -p "$tmpdir"
    tar xf "$ARCHIVE_FILE" -C "$tmpdir" 2>/dev/null

    local all_match=true
    local compared=0

    while IFS= read -r -d '' src; do
        local rel_path="${src#$tmpdir/}"
        local restored_path="$RESTORE_DIR/$rel_path"

        if [ ! -f "$restored_path" ]; then
            log_warn "未找到恢复文件: $rel_path"
            continue
        fi

        local src_sha=$(sha256sum "$src" | awk '{print $1}')
        local rst_sha=$(sha256sum "$restored_path" | awk '{print $1}')
        local fname=$(basename "$src")

        if [ "$src_sha" = "$rst_sha" ]; then
            log_ok "$fname: SHA256 一致"
        else
            log_fail "$fname: SHA256 不一致"
            all_match=false
        fi
        compared=$((compared + 1))
    done < <(find "$tmpdir" -type f -print0)

    rm -rf "$tmpdir"

    if [ "$compared" -eq 0 ]; then
        log_warn "无可比对文件"
    elif $all_match; then
        log_ok "所有 $compared 个文件级别比对通过"
        report "文件比对: 全部通过 ($compared 文件)"
    else
        log_warn "部分文件比对失败 (共 $compared 文件)"
        report "文件比对: 部分失败 ($compared 文件)"
    fi
}

unload_tape() {
    log_info "卸载磁带..."

    mt -f "$TAPE_DRIVE" rewind 2>/dev/null || true
    sleep 1

    if mtx -f "$CHANGER" unload "$TAPE_SLOT" "$TAPE_DRIVE_IDX" 2>/dev/null; then
        log_ok "磁带已卸载回槽位 $TAPE_SLOT"
    else
        log_warn "mtx unload 失败，尝试 mt offline..."
        mt -f "$TAPE_DRIVE" offline 2>/dev/null || true
    fi

    report "磁带已卸载"
}

generate_report() {
    echo ""
    log_info "测试报告: $REPORT_FILE"
    {
        echo "========================================"
        echo "测试环境"
        echo "========================================"
        echo "主机名: $(hostname)"
        echo "内核版本: $(uname -r)"
        echo "完成时间: $(date)"
        echo "强制落盘: $FORCE_SYNC"
        echo "驱动器: $TAPE_DRIVE"
        echo "磁带: 槽位 $TAPE_SLOT ($TAPE_VOLTAG)"
        echo "恢复目录: $RESTORE_DIR"
    } >> "$REPORT_FILE"
}

cleanup() {
    log_info "清理临时文件..."
    rm -f /tmp/vtl_test_orig_sha.txt
    rm -f /tmp/vtl_test_journalctl.log
    rm -rf /tmp/vtl_test_orig_extract_*
    log_info "归档文件保留: $ARCHIVE_FILE, $RESTORED_FILE"
}

# ===== 主流程 =====
main() {
    # 解析参数
    for arg in "$@"; do
        case $arg in
            --cache) FORCE_SYNC=false ;;
            --help|-h) usage ;;
            *) log_warn "未知参数: $arg"; usage ;;
        esac
    done

    banner

    echo "VTL 真实数据读写测试报告" > "$REPORT_FILE"
    echo "========================================" >> "$REPORT_FILE"

    check_deps
    check_devices
    load_tape
    package_logs
    write_to_tape
    read_from_tape

    if verify_checksum; then
        restore_to_disk
        file_level_compare
    else
        log_fail "SHA256 校验失败，跳过恢复和文件比对"
    fi

    unload_tape
    generate_report
    cleanup

    echo ""
    echo -e "${GREEN}╔══════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║          测试完成                            ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════╝${NC}"
    echo ""
    log_info "模式: $( $FORCE_SYNC && echo '强制落盘 (sync + drop_caches)' || echo '缓存模式')"
    log_info "恢复目录: $RESTORE_DIR"
    log_info "测试报告: $REPORT_FILE"
}

trap cleanup EXIT
main "$@"
