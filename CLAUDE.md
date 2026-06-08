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

### Kylin 4.19 特殊处理
- st 驱动在探测时发送 TUR/READ POSITION/SPACE 等命令，对空驱动器返回 NOT_READY
  会触发 "Device offlined - not ready after error recovery" → SDEV_OFFLINE → SG_IO 全部 ENXIO
- **解决方案**：空驱动器 TUR/READ/REWIND/SPACE/LOAD 全部返回 SAM_STAT_GOOD
  - TUR → GOOD（防止 st 离线）
  - READ → GOOD + 0 字节（模拟空白磁带）
  - REWIND → GOOD（no-op）
  - SPACE → GOOD（no-op）
  - LOAD_UNLOAD(load=1) → GOOD（no-op）
- READ POSITION 不返回 NOT_READY，总是返回有效数据
- 延迟启动 + 串行化 SCSI 扫描避免 st_probe 竞态（`vtl_main.c` 参数）

### 磁带数据存储
- 磁带文件：`*.vtltape`，VTL_BLOCK_HEADER(16B) + 压缩数据
- 压缩支持：LZO / zlib，VTL_COMP_NONE 表示不压缩（标签写入即用此模式）
- 标签侧文件：`*.vtlfm`，存储文件标记字节偏移量（VTL_FM_MAGIC="VTLF"）
- 密度元数据侧文件：`*.vtlmeta`，存储密度/压缩标志（VTL_META_MAGIC="VTLM"）

### 文件标记持久化 (2026-06-08 新增)
- 侧文件 `*.vtlfm`：VTL_FM_MAGIC 头 + big-endian uint64 偏移数组
- 写入：WRITE FILEMARKS → 记录 `tape->position`，设置 `meta_dirty=true`
- 持久化：磁带卸载时 (`vtl_tape_unload` / `changer_move_medium` 源驱动器路径) 调用 `vtl_tape_save_metadata`
- 恢复：磁带加载时 (`vtl_tape_load` / `changer_move_medium` 驱动器目标路径) 调用 `vtl_tape_load_metadata`
- SPACE code 1/2：基于 `filemark_offsets` 数组导航
- READ POSITION：二分查找 `filemark_offsets` 报告当前文件号
- 位置推进：WRITE FILEMARKS 后将 `position` 推进到 `i_size_read()`（文件数据末尾）

## 关键代码位置

| 功能 | 文件 |
|------|------|
| SCSI 命令分发 | `kernel/src/vtl_scsi.c:vtl_scsi_queuecommand` |
| TUR/READ/REWIND/SPACE/LOAD 处理 | `kernel/src/vtl_scsi.c` (1098-1320) |
| READ POSITION + 文件号 | `kernel/src/vtl_scsi.c:1650` |
| READ CAPACITY (10/16) | `kernel/src/vtl_scsi.c:1136,1185` |
| 块读写/压缩 | `kernel/src/vtl_tape.c:597,711` |
| 文件标记持久化 | `kernel/src/vtl_tape.c:523-680` |
| SPACE + 文件标记导航 | `kernel/src/vtl_tape.c:988-1069` |
| tape->filemark_offsets 数组 | `kernel/include/vtl.h:200-217` |
| 文件标记头文件定义 | `kernel/include/vtl.h:128-143` |
| 用户态标签命令 | `userspace/src/label.rs` |
| 标签 CLI 集成 | `userspace/src/main.rs:label_tape_in_library` |
| Web 登录/认证 | `userspace/src/web.rs:1161` |

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

## 构建和测试

```bash
# 完整安装
./install.sh --enable

# 仅内核模块
cd kernel && make

# 仅用户态
cd userspace && cargo build --release

# 仅 Web UI (需要 Node >= 18)
cd userspace/web && npm install && npm run build

# 集成测试
# 见 TEST.md
```

## 部署注意事项

### Node.js 版本
- Web UI 构建需要 Node >= 18 (vite 6 + vue-tsc 2.x)
- Node 12 无法构建，install.sh 自动跳过，使用 legacy HTML
- 可在开发机构建后把 `userspace/web/dist/` 复制到部署服务器

### 模块重载
- `./install.sh --enable` 首次安装，后续用户态更新用 `./install.sh --no-reload`
- 内核更新默认需要 rmmod + 重启
- 维护窗口可用 `VTL_FORCE_RELOAD=1 ./install.sh --no-reboot`

### 标签写入
```bash
# 给磁带打 ANSI/IBM 标准标签
vtladm label mytape                    # 默认 ANSI
vtladm label mytape -f ibm -v MYVOL   # IBM 格式，指定卷号
```

## 已知限制

1. Web UI 构建需要 Node >= 18，低版本自动跳过
2. st 驱动在 Kylin 4.19 有多种触发离线的路径，空驱动器大部分命令已改为返回 GOOD
3. 文件标记在 REWIND 后追加可导致 `filemark_offsets` 数组乱序（需后续加排序）
4. 侧文件扩展名 `.vtlfm` (filemark) 与 `.vtlmeta` (密度) 分离，尚未合并
5. `vtl_tape_load` 函数当前未使用（代码路径统一走 `changer_move_medium`）
