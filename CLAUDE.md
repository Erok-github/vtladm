# CLAUDE.md

## 项目概述

vtladm 是一个基于 Linux 内核模块的虚拟磁带库 (VTL) 实现。内核模块 `vtl.ko` 创建 SCSI 机械手和磁带机设备，用户态 `vtladm` (Rust) 提供 CLI 和 Web 管理。备份软件通过本地 SG/ST 设备或 iSCSI 导出来访问虚拟磁带库。

- **内核**：Linux 4.18–6.10 (C, 内核模块)
- **用户态**：Rust (CLI + Web, 目标 x86_64/aarch64)
- **前端**：Vue3 + Naive UI + Vite
- **部署**：`./install.sh --enable` → `/opt/vtladm` + systemd

## 架构要点

### SCSI 设备模型
- 每个 VTL 实例 = 1 个 SCSI host
- LUN 0 = 机械手 (Medium Changer, 0x08)
- LUN 1..N = 磁带机 (Sequential Access, 0x01)
- 所有 SCSI 命令在 `kernel/src/vtl_scsi.c:vtl_scsi_queuecommand` 路由

### SCSI EH (Error Handling) — 2026-06-12 新增
- `kernel/src/vtl_main.c`: `vtl_eh_abort` / `vtl_eh_device_reset` / `vtl_eh_host_reset`
- 三个处理器均返回 `SUCCESS`（VTL 命令永不挂起）
- **作用**：阻止 Kylin 4.19 st 驱动的 "Device offlined - not ready after error recovery" 链式反应
- mhVTL 也有这三个回调，没有就会触发 SCSI 中间层升级到 bus reset → st 离线

### st 驱动 DMA Alignment — 2026-06-12 新增
- `kernel/src/vtl_scsi.c`: `vtl_slave_alloc` + `vtl_slave_configure` 中调用 `blk_queue_dma_alignment(sdev->request_queue, 511)`
- 设置 512B DMA 对齐，使 st 驱动 `try_direct_io=1` 生效
- 未设置时 st 报告 `"try direct i/o: no (alignment 4 B)"`，所有读写被缓冲绕过 SCSI

### Kylin 4.19 st 驱动行为
- **SILI=0 bug**：Kylin 4.19 st 在变长块模式下发送 `SILI=0`，忽略 SCSI residual
- 短块读取时 st 用缓冲区旧数据填充到请求大小
- **解决方案** (`kernel/src/vtl_scsi.c:vtl_handle_read`)：
  - TUR/READ/REWIND/SPACE/LOAD 对空驱动器返回 SAM_STAT_GOOD
  - 短块读取时用 `memset` zero-padding 补齐（避免旧缓冲数据污染）
- **mt 命令恢复**：MOVE MEDIUM 后第一个 mt 命令返回 ENXIO，第二个成功（st 错误恢复周期）
- **install.sh**：warmup 时做 `mt status` + 双重 `mt rewind` 确保 st 恢复

### 磁带数据存储
- 磁带文件：`*.vtltape`，VTL_BLOCK_HEADER(16B) + 压缩数据
- 压缩支持：LZO / zlib，VTL_COMP_NONE 表示不压缩（标签写入即用此模式）
- **Filemark 侧文件**：`*.vtlfm`，VTL_FM_MAGIC="VTLF"
- **元数据侧文件**：`*.vtlmeta`，VTL_META_MAGIC="VTLM"，v2 格式增加 `used` 字段

### EOD (End of Data) 持久化 — 2026-06-12 新增
- `kernel/src/vtl_tape.c` 和 `kernel/include/vtl.h`
- `.vtlmeta` v2 格式：magic(4) + version(2) + density(1) + flags(1) + used(8) + reserved(8) = 24B
- `vtl_meta_write(path, density, flags, used)` — 写入 used 字段
- `vtl_meta_read(path, &density, &flags, &used)` — 读取，v1 兼容（used=0）
- **保存时机**：`vtl_tape_unload`、`changer_move_medium`（驱动卸载路径）、`vtl_changer_clear_media`（驱动清理循环）
- **恢复时机**：`vtl_tape_open_existing` 时从 sidecar 恢复 `meta.used`
- 模块重载后 meta.used 不再丢失

### 文件标记持久化
- 侧文件 `*.vtlfm`：VTL_FM_MAGIC 头 + big-endian uint64 偏移数组
- 写入：WRITE FILEMARKS → 记录 `tape->position`，设置 `meta_dirty=true`
- 持久化：磁带卸载时 (`vtl_tape_unload` / `changer_move_medium` / `vtl_changer_clear_media`) 调用 `vtl_tape_save_metadata`
- 恢复：磁带加载时 (`vtl_tape_load` / `changer_move_medium` 驱动器目标路径) 调用 `vtl_tape_load_metadata`
- SPACE code 1/2：基于 `filemark_offsets` 数组导航
- READ POSITION：二分查找 `filemark_offsets` 报告当前文件号

## 关键代码位置

| 功能 | 文件 |
|------|------|
| SCSI 命令分发 | `kernel/src/vtl_scsi.c:vtl_scsi_queuecommand` |
| SCSI EH handlers | `kernel/src/vtl_main.c` (vtl_eh_abort/device_reset/host_reset) |
| DMA alignment (slave_alloc) | `kernel/src/vtl_scsi.c:2244` |
| READ 零填充 (SILI=0) | `kernel/src/vtl_scsi.c:1089-1093` |
| TUR/READ/REWIND/SPACE/LOAD 处理 | `kernel/src/vtl_scsi.c` (1098-1320) |
| READ POSITION + 文件号 | `kernel/src/vtl_scsi.c:1650` |
| READ CAPACITY (10/16) | `kernel/src/vtl_scsi.c:1136,1185` |
| 块读写/压缩 | `kernel/src/vtl_tape.c:597,711` |
| meta.used 持久化 (vtl_meta_write/read) | `kernel/src/vtl_tape.c:123-210` |
| 文件标记持久化 (vtl_tape_save/load_metadata) | `kernel/src/vtl_tape.c:523-680` |
| SPACE + 文件标记导航 | `kernel/src/vtl_tape.c:988-1069` |
| changer_move_medium (含 meta 保存) | `kernel/src/vtl_tape.c:1182` |
| clear_media (含 meta 保存) | `kernel/src/vtl_tape.c:473` |
| tape->filemark_offsets 数组 | `kernel/include/vtl.h:200-217` |
| vtl_meta_header 定义 | `kernel/include/vtl.h:103-109` |
| 用户态标签命令 | `userspace/src/label.rs` |
| 标签 CLI 集成 | `userspace/src/main.rs:label_tape_in_library` |
| Web 登录/认证 | `userspace/src/web.rs:1161` |
| 自动化部署脚本 | `scripts/deploy-and-test.sh` |

## 空驱动器的 SCSI 行为总结

| 命令 | 有磁带 | 无磁带 |
|------|--------|--------|
| TUR (0x00) | GOOD | GOOD |
| READ (0x08/28/A8) | GOOD (正常读) | GOOD (0字节) |
| WRITE (0x0A/2A/AA) | GOOD | CHECK CONDITION |
| REWIND (0x01) | GOOD | GOOD (no-op) |
| SPACE (0x11) | GOOD | GOOD (no-op) |
| LOAD_UNLOAD load=1 | GOOD | GOOD (no-op) |
| READ POSITION (0x34) | GOOD | GOOD (BPU bit) |
| WRITE FILEMARKS (0x10) | GOOD | CHECK CONDITION |

## 部署

### st 驱动配置
```bash
# /etc/modprobe.d/vtl-st.conf (由 install.sh 自动部署)
options st try_direct_io=1 try_rdio=1 try_wdio=1 buffer_kbs=32
```

### 自动化部署
```bash
# 同步源码 → 编译 vtl.ko → 重载模块 → 验证测试 (默认目标 192.168.5.63)
./scripts/deploy-and-test.sh root@192.168.5.63

# 跳过编译 (假设 vtl.ko 已是最新)
./scripts/deploy-and-test.sh root@192.168.5.63 --quick
```

### 手动安装
```bash
# 完整安装
./install.sh --enable

# 仅内核模块
cd kernel && make

# 仅用户态
cd userspace && cargo build --release

# 仅 Web UI (需要 Node >= 18)
cd userspace/web && npm install && npm run build
```

### 模块重载
- `./install.sh --enable` 首次安装，后续用户态更新用 `./install.sh --no-reload`
- 内核更新默认需要 rmmod + 重启
- 维护窗口可用 `VTL_FORCE_RELOAD=1 ./install.sh --no-reboot`

## 手动验证测试

```bash
# 1. 加载磁带
mtx -f /dev/sg2 load 1 0

# 2. 恢复 st (第一次 ENXIO 正常)
mt -f /dev/st0 rewind; mt -f /dev/st0 rewind

# 3. 写入真实文件
dd if=/var/log/messages-20260611 of=/dev/st0 bs=32k

# 4. 回读 (count 必须匹配写入块数)
mt -f /dev/st0 rewind
dd of=/tmp/readback if=/dev/st0 bs=32k count=26699

# 5. 验证内容
cmp -n $(stat -c%s /var/log/messages-20260611) \
    /var/log/messages-20260611 /tmp/readback \
    && echo "✓ 内容完全匹配"
```

## 已知限制

1. Web UI 构建需要 Node >= 18，低版本自动跳过
2. **Kylin 4.19 st 驱动 SILI=0**：短块读取时最后不完整块被零填充（32KB 块中最多 26KB 零填充）。内容正确，但逐字节 cmp 会因尾部零填充失败。tar 等实际应用不受影响
3. st 驱动 MOVE MEDIUM 后首个 mt 命令 ENXIO，第二个恢复正常（install.sh warmup 已处理）
4. 文件标记在 REWIND 后追加可导致 `filemark_offsets` 数组乱序（需后续加排序）
5. 侧文件扩展名 `.vtlfm` (filemark) 与 `.vtlmeta` (EOD/密度) 分离，尚未合并
6. `vtl_tape_load` 函数当前未使用（代码路径统一走 `changer_move_medium`）
7. `sg_dd` / `sg_dd --verify` 不兼容（使用磁盘 LBA 寻址 CDB，非磁带格式）
