# ZFS ZVOL 后端存储支持

> 状态：设计草案（技术方向评审版）  
> 版本：v0.2  
> 变更：相对 v0.1 拆分路径/元数据、修正 ZFS 语义、对齐现有 vtladm 双入口模型  
> 目标：在现有 **文件镜像**（`.vtltape`）上增加 **ZFS ZVOL** 后端，通过 `vtl.conf` 切换，默认行为不变

---

## 1. 目标与非目标

### 1.1 目标

- **零配置降级**：`tape_backend=file`（默认）时，行为与当前 `vtl_tape.c` + `vtladm` 完全一致。
- **职责清晰**：ZVOL 的 **创建/销毁/快照/扩容** 由 **用户态 vtladm** 通过 `zfs` CLI 完成；内核 **只打开已有块设备并读写**（与今天「用户态建文件、内核打开」的分工一致）。
- **透明收益**（仅 zvol 模式）：ZFS 快照、`zfs send/recv`、校验和、池级压缩；可选与 bakvtl 任务钩子联动。
- **可运维**：配置项可同步到 `vtl.ko` 模块参数（与现有 `tape_dir` sysfs 同步方式一致）。

### 1.2 非目标（v0.2 不解决）

- 不替代 ZFS 池规划（mirror/raidz/draid 由管理员预先创建）。
- 不要求所有 Kylin/openEuler 安装包自带 ZFS 内核模块（见 §2.4 前置条件）。
- 不把「mhVTL 路径兼容」当作验收项——验收以 **SCSI/机械手/驱动器语义** 与文件模式一致为准。

---

## 2. 前置条件与平台

| 项 | 要求 |
|----|------|
| 用户态 | `zfs` / `zpool` 可用，版本与目标池一致 |
| 内核 | ZFS 已加载，`/dev/zvol/<pool>/...` 可访问 |
| vtl.ko | 4.18+（现有要求）；ZVOL 路径在目标内核上验证 `filp_open` + 大块顺序写 |
| 共存 | 与 LIO/pscsi（`vtladm-iscsi`）同机无架构冲突；注意 **root 权限** 执行 `zfs` |
| 冗余 | 生产池建议 mirror/raidz；本文不规定池拓扑 |

---

## 3. 配置模型（v0.2 核心修订）

### 3.1 为何拆分多个路径

当前实现中：

- **`tape_dir`**：用户态库布局根（`{tape_dir}/{库名}/{磁带名}.vtltape`）、`link_kernel_tapes` 硬链到 **扁平根**、`.vtlmeta` 旁路文件。
- **内核**：仅按 **`{tape_dir}/{name}.vtltape`** 打开（`vtl_format_tape_path`），与 DB 中 `image_path` 可能不一致。

ZVOL 模式下 **不能把 `tape_dir` 设为 `/dev/zvol/...`**（设备目录上无法 `create_dir_all`、无法存放库子目录与 `.vtlmeta`）。因此 v0.2 **强制拆分**：

| 配置项 | 文件模式 | ZVOL 模式 | 说明 |
|--------|----------|-----------|------|
| `tape_backend` | `file` | `zvol` | 总开关 |
| `tape_dir` | 镜像 + 布局根 | **仍为普通文件系统目录** | 库子目录、exports、巡检统计 |
| `meta_dir` | 默认同 `tape_dir` | **必填（或默认同 `tape_dir`）** | 仅存放 `{name}.vtlmeta` |
| `zvol_dataset` | — | 如 `tank/vtl-tapes` | ZFS 父 dataset（文件系统 dataset，非卷） |
| `zvol_device_root` | — | 如 `/dev/zvol/tank/vtl-tapes` | 内核与 DB `image_path` 使用的设备前缀 |
| `zfs_pool` | — | 如 `tank` | 校验池存在、配额、`zfs stats` |

### 3.2 vtl.conf 示例

```ini
# --- 存储后端 ---
tape_backend = file          # file | zvol

# --- 通用：始终为「普通目录」---
tape_dir = /opt/vtladm/var/tapes
meta_dir = /opt/vtladm/var/tapes    # {meta_dir}/{name}.vtlmeta

# --- ZFS（仅 zvol）---
zfs_pool = tank
zvol_dataset = tank/vtl-tapes
zvol_device_root = /dev/zvol/tank/vtl-tapes
zfs_volblocksize = 128K
zfs_compression = lz4
zfs_export_dir = /opt/vtladm/var/tapes/exports

# 快照轮转：由独立 timer 触发，策略与保留数成对
zfs_snap_timer = daily           # daily | weekly（与 systemd timer 一致）
zfs_snap_keep = 7,4,12           # 仅当 policy 启用多档时：日,周,月 保留份数

# SCSI 与 ZFS 块对齐（建议同时配置）
vtl_block_size = 128K            # 与 zfs_volblocksize 一致，避免写放大

# --- 文件模式示例（默认）---
# tape_backend = file

# --- ZVOL 模式示例 ---
# tape_backend = zvol
# zfs_pool = tank
# zvol_dataset = tank/vtl-tapes
# zvol_device_root = /dev/zvol/tank/vtl-tapes
```

### 3.3 配置加载与内核同步

```
vtl.conf
   └── vtladm 解析、校验
          ├── file  → 无 ZFS 操作
          └── zvol  → zpool list / dataset 存在性检查
                      zfs create 父 dataset（安装/ zfs init 时一次）
                      写 DB tapes.image_path = {zvol_device_root}/{name}
                      写 meta = {meta_dir}/{name}.vtlmeta

   └── 同步内核（与今日 tape_dir 相同机制）
          /sys/module/vtl/parameters/tape_backend
          /sys/module/vtl/parameters/tape_dir   ← zvol 模式下指向 zvol_device_root
          /sys/module/vtl/parameters/meta_dir   ← 新增，供 vtl_meta_* 使用
```

**运行中切换 `tape_backend` 或设备根路径**：必须先 **unload 所有已装载磁带** 并 reload `vtl.ko`（或 `vtl-kernelctl reload`）；禁止对正在读写的卷静默切换。

### 3.4 配置校验（zvol 模式强制）

- `tape_backend=zvol` 时：**拒绝**启用 vtladm 全局压缩或内核压缩（`compression.c`）；MODE SELECT 压缩位忽略或返回 CHECK CONDITION（实现时二选一，文档建议 **配置层强制关闭**）。
- `zfs_volblocksize` 与 `vtl_block_size` 不一致时：**启动告警**，建议两者相等且为 2 的幂。
- `check_quota`：改为基于 **`zpool available`**（及可选 `refreservation` 策略），不再仅看目录所在文件系统。

---

## 4. 架构

### 4.1 存储视图

```
┌─────────────────────────────────────────────────────────────┐
│ vtladm (用户态) — 权威：创建/删除/快照/配额/DB image_path      │
│   file:  File::create + set_len  →  {tape_dir}/{lib}/{name}.vtltape │
│   zvol:  zfs create -V …          →  DB 记录块设备路径          │
│   共用:  {meta_dir}/{name}.vtlmeta（两种模式相同）              │
│   共用:  link_kernel_tapes：zvol 下改为「校验设备节点存在」      │
└────────────────────────────┬────────────────────────────────┘
                             │ DB + sysfs 参数
                             ▼
┌─────────────────────────────────────────────────────────────┐
│ vtl.ko — 仅打开已有后端，不在 zvol 模式调用 vfs_fallocate 建卷 │
│   file: filp_open("{tape_dir}/{name}.vtltape")              │
│         kernel_read/write，.vtlmeta 在 meta_dir 或旁路规则   │
│   zvol: filp_open("{zvol_device_root}/{name}", O_RDWR[|O_DIRECT]) │
│         kernel_read/write；禁止 vtl_tape_create 内 fallocate │
└────────────────────────────┬────────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
         普通文件系统                    ZFS ZVOL
    .vtltape + .vtlmeta              /dev/zvol/.../NAME
```

**说明**：实现上优先 **统一使用 `struct file *` + filp_open("/dev/zvol/...")`**，与现有 `vtl_tape` 代码结构一致；`blkdev_get_by_path` 可作为备选，不在 v0.2 首选路径中混用两套 API。

### 4.2 磁带 ↔ 存储映射

| 项目 | 文件模式 | ZVOL 模式 |
|------|----------|-----------|
| 创建（权威） | `vtladm tape create` → 文件 + vtlmeta | `vtladm tape create` → `zfs create -V` |
| 内核首次 open | 可 `vtl_tape_create` fallocate（遗留） | **仅** `vtl_tape_open_existing`；create 返回错误并打日志 |
| DB `image_path` | `{tape_dir}/{lib}/{name}.vtltape` | `{zvol_device_root}/{name}` |
| 元数据 | `{meta_dir}/{name}.vtlmeta` | 同左（**不在** `/dev/zvol` 下） |
| 删除 | unlink + DB | `zfs destroy` + DB；销毁前可选 `@pre-delete` 快照 |
| 容量 | `set_len` / 文件大小 | `zfs set volsize=` / `zfs resize` |
| 内核读写 | `kernel_read/write` on file | 同左 on block device file |

### 4.3 与现有命令对照

| 现有命令 | 文件模式行为 | ZVOL 模式行为 |
|----------|--------------|---------------|
| `vtladm tape create` | 创建 `.vtltape` | `zfs create -V`；写 DB 设备路径 |
| `vtladm tape snapshot` | **文件拷贝** `{tape}_{snap}.vtltape` | **禁用或打印提示**，改用 `vtladm zfs snapshot` |
| `vtladm zfs snapshot`（新增） | N/A | `zfs snapshot` |
| `vtladm tape init` | 截断/重建文件 | `zfs destroy` + `zfs create -V`（**销毁该卷全部 ZFS 快照**） |
| `vtladm tape delete` | 删文件 | `zfs destroy` |

---

## 5. 元数据（.vtlmeta）

内核与用户态 today 通过 **旁路文件** 保存 density / 压缩标志（`vtl_meta_header`），路径由磁带镜像路径推导；v0.2 规定：

- **所有模式**：`vtl_format_meta_path(name)` → `{meta_dir}/{name}.vtlmeta`（**不再**依赖 `.vtltape` 后缀替换）。
- 内核模块新增 `meta_dir` 模块参数；`vtl_meta_read/write` 只访问该目录。
- 未来可选：ZFS user properties 存 density，与 `.vtlmeta` 二选一（P2 以后），v0.2 以 **meta_dir 文件** 为准，改动最小。

---

## 6. 快照

### 6.1 ZFS 语义（必读）

- ZFS **不支持**快照重命名。v0.1「将 pre 重命名为 ok」**作废**。
- 推荐状态机：

```
备份开始 → zfs snapshot …@…-pre
成功     → zfs snapshot …@…-ok（新快照）；zfs destroy …@…-pre（可选）
失败     → zfs destroy …@…-pre
```

### 6.2 触发来源

| 来源 | 场景 | 实现归属 |
|------|------|----------|
| 计划任务 | `vtl-zfs-snapshot.timer`（**每日**或每周，与 `zfs_snap_timer` 一致） | vtladm 脚本 |
| 手动 | `vtladm zfs snapshot TAPE` | vtladm |
| 删除保护 | `tape delete` 前 `@…-pre-delete` | vtladm |
| 备份任务 | 任务前/后 pre/ok | **可选**；由 bakvtl 调 vtladm API/脚本（**非 v0.2 阻塞项**） |

**注意**：`vtl-patrol.timer` 为 **巡检**，不替代快照 timer；二者职责分离。

### 6.3 命名与 tag

```
tank/vtl-tapes/TAPE001@TAPE001-20260602-143000-ok
tank/vtl-tapes/TAPE001@TAPE001-20260602-143000-pre
```

格式：`{zvol}@{barcode}-{YYYYMMDD}-{HHmmss}-{tag}`  

tag：`pre` | `ok` | `admin` | `rotate` | `pre-delete` | `export`

### 6.4 保留策略

```ini
zfs_snap_timer = daily
zfs_snap_keep = 7,4,12    # 日、周、月 各保留数量（仅 rotate 类快照参与）
```

轮转（`vtl-zfs-snapshot.sh`）：

1. `zfs list -t snapshot -r {zvol_dataset}`，筛选 `tag=rotate` 或命名约定。
2. 按日/周/月桶保留 `zfs_snap_keep` 对应份数，其余 `zfs destroy`。
3. **不**在每小时 patrol 中做轮转，避免与 `zfs_snap_timer` 冲突。

### 6.5 恢复策略

| 手段 | 适用 | 注意 |
|------|------|------|
| `zfs rollback` | 离线维护窗口 | 丢弃快照点之后写入；**不推荐**对正在 load 的磁带执行 |
| `zfs clone` + 换 DB `image_path` | 在线恢复验证 | 推荐集成测试用法 |
| `zfs send/recv` | 异地/归档 | 见 §10 |

---

## 7. 压缩

### 7.1 父 dataset 与子 ZVOL

```bash
# 父级：文件系统 dataset（挂载点可选，用于管理）
zfs create -o compression=lz4 tank/vtl-tapes

# 每盘磁带：ZVOL，块大小在卷上设定
zfs create -V 10G -o volblocksize=128k -o refreservation=none \
  tank/vtl-tapes/TAPE001
```

勿将 `recordsize=128k` 当作 ZVOL 块大小；**volblocksize** 才影响块设备 I/O。

### 7.2 与内核 compression.c

```
应用 ── SCSI WRITE ── vtl.ko [compression.c 必须关闭] ── ZVOL ── ZFS lz4
```

`tape_backend=zvol` 时：配置与 MODE SELECT **均不启用**内核压缩；否则双重压缩，CPU 浪费且 ZFS 收益接近零。

---

## 8. 块大小对齐

| 层级 | 默认 | 说明 |
|------|------|------|
| VTL SCSI `block_size` | 32K（现网） | 建议 zvol 部署时改为 **128K**（`vtl.conf` + MODE SELECT） |
| ZVOL `volblocksize` | 128K | `zfs create -V` 时设定 |
| 规则 | `volblocksize >= block_size`（2 的幂） | 否则单次 WRITE 跨多块 → 写放大 |

---

## 9. 直接 I/O（P4 优化项）

文件模式：`kernel_read/write` 经 page cache。ZVOL 模式可选 `filp_open(..., O_DIRECT)` + `IOCB_DIRECT`，减轻 page cache + ARC 双层缓存。

- **收益**：大块顺序写吞吐（预估 15–30%，需 Kylin 实测）。
- **风险**：部分 4.19 厂商内核对块设备 `O_DIRECT` 行为不一致；**P1 可先用默认 buffered，P4 再开关**。

---

## 10. 空间与配额

### 10.1 稀疏卷

```bash
zfs create -V 10G -o refreservation=none tank/vtl-tapes/TAPE001
```

### 10.2 回收

| 操作 | 命令 | 说明 |
|------|------|------|
| 删除磁带 | `zfs destroy` | 空间归还池；**不需要** `blkdev_issue_discard` |
| 擦除/init | destroy + create -V | **删除该卷全部快照** |
| 扩容 | `zfs set volsize=` | 磁带容量变更需 DB `capacity_bytes` 同步 |

### 10.3 配额

`vtladm tape create` 前：`check_quota` 改为检查 `zpool available`（及策略性 reservation），避免稀疏卷撑满池。

---

## 11. 并发与锁

与 v0.1 相同要点，补充：

- **删除磁带**：必须先内核 unload（`tape init` / unload），避免 `vtl_tape.lock` 与 `zfs destroy` 竞态。
- **`zfs send`**：不阻塞源卷继续写入；归档完成后再 `destroy` 旧快照。

---

## 12. 归档

### 12.1 本地

```bash
zfs send tank/vtl-tapes/TAPE001@TAPE001-20260602-120000-export \
  > /opt/vtladm/var/tapes/exports/TAPE001.zfs
```

### 12.2 恢复

```bash
# 目标不存在
zfs receive tank/vtl-tapes/TAPE001 < TAPE001.zfs

# 目标已存在（覆盖）
zfs receive -F tank/vtl-tapes/TAPE001 < TAPE001.zfs
```

### 12.3 `vtladm zfs export`（新增）

1. `zfs snapshot …@…-export`
2. `zfs send`（可管道 `gzip`）→ `zfs_export_dir`
3. 校验 checksum（可选）
4. 按保留策略清理 `rotate` / 过期 `export` 快照

---

## 13. 内核改造清单（修订）

| 函数 | file | zvol |
|------|------|------|
| `vtl_tape_create` | 现状 | **返回 -EINVAL**，日志提示由 vtladm 建卷 |
| `vtl_tape_open_existing` | `filp_open(.vtltape)` | `filp_open(/dev/zvol/.../name)` |
| read/write | `kernel_read/write` | 同左；P4 可选 O_DIRECT |
| get_size | `vfs_llseek` | `i_size_read` / `vfs_llseek` on block file |
| meta | `{meta_dir}/{name}.vtlmeta` | 同左 |

新增模块参数：`tape_backend`、`meta_dir`；`tape_dir` 在 zvol 模式下语义为 **设备根**（与 `zvol_device_root` 同步）。

---

## 14. 用户态改造清单（修订）

### 14.1 新增 `vtladm zfs` 子命令

```bash
vtladm zfs init              # 创建父 dataset、检查池
vtladm zfs snapshot TAPE …   # ZFS 快照
vtladm zfs list-snap TAPE
vtladm zfs cleanup [--all]   # 按 zfs_snap_keep 轮转
vtladm zfs export TAPE
vtladm zfs import TAPE FILE   # 内部调用 receive [-F]
vtladm zfs stats
```

### 14.2 `tape` 子命令

- `create` / `delete` / `init`：走 §4.2 权威路径。
- `snapshot`：file 模式保留；zvol 模式 **拒绝并提示** 使用 `vtladm zfs snapshot`。

### 14.3 systemd

- `vtl-zfs-snapshot.timer`：周期与 `zfs_snap_timer` 一致（**不要**挂在 `vtl-patrol.timer` 上）。
- patrol 仅增加：池空间、`zvol` 列表、设备节点与 DB 一致性巡检。

---

## 15. 安装脚本

```bash
if [ "$tape_backend" = "zvol" ]; then
  command -v zfs >/dev/null || exit 1
  zpool list "$zfs_pool" >/dev/null || exit 1
  zfs create -o compression="${zfs_compression:-lz4}" "$zvol_dataset" 2>/dev/null || true
  # 不在 install 时为每盘磁带 create -V；由 vtladm tape create 完成
fi
```

---

## 16. 兼容性矩阵

| 功能 | file | zvol |
|------|------|------|
| SCSI 机械手/驱动器语义 | ✓ | ✓（与路径无关） |
| iSCSI pscsi 导出 | ✓ | ✓ |
| `vtladm tape snapshot`（文件拷贝） | ✓ | ✗（改用 zfs） |
| 内核 compression.c | 可选 | **禁止** |
| 远程复制 | rsync/scp | zfs send/recv |

---

## 17. 风险与限制

| 风险 | 级别 | 缓解 |
|------|------|------|
| 双入口创建（内核 vs vtladm） | 高 | zvol 禁止内核 create；文档 + 测试 |
| meta 写入 /dev/zvol | 高 | 强制 `meta_dir` |
| 运行中切换 backend | 高 | 要求 unload + reload |
| O_DIRECT 兼容性 | 中 | P4 实测，P1 默认 buffered |
| `tape init` 误操作 | 高 | 文档警告；可选 destroy 前自动快照 |
| 池满 | 中 | 配额改 zpool available |
| ZFS 未安装 | 中 | 安装前检查；降级保持 file |

---

## 18. 实施优先级（修订）

| 阶段 | 内容 | 验收 |
|------|------|------|
| **P0** | 配置拆分、`meta_dir`、校验、sysfs 同步、`zfs init` | 配置错误可拒绝启动；file 模式无回归 |
| **P1** | 内核 zvol **只读打开** + meta_dir；vtladm `tape create/delete` 走 zfs | 单盘 mtx/tar 读写正确 |
| **P2** | `vtladm zfs *`、禁用 `tape snapshot`、配额/zpool、block_size 对齐 | 与 file 模式 SCSI 行为一致 |
| **P3** | 快照 timer、export/import、保留策略 | send/recv 往返成功 |
| **P4** | O_DIRECT、性能与 Kylin 全量回归 | 吞吐达标、无双重缓存问题 |

**阻塞关系**：未完成 P0（路径/meta 模型）不得开始 P1 内核改造。

---

## 19. 测试计划（修订）

| 测试 | 方法 | 预期 |
|------|------|------|
| 创建/删除 | `vtladm tape create/delete` | ZVOL 与 DB、/dev/zvol 一致 |
| meta | 改 density 后 reload | `.vtlmeta` 在 `meta_dir`，不在 /dev |
| 读写 | mtx + tar 到大块 | 数据一致 |
| 快照 | `vtladm zfs snapshot` + `zfs destroy pre` | **无 rename** |
| 恢复 | `zfs clone` 或维护窗口 `rollback` | 文档场景内数据正确 |
| init | `tape init` | 旧快照消失、新空卷 |
| 命令冲突 | `tape snapshot` on zvol | 明确报错 |
| 并发 | 多驱动器同时写 | 无损坏 |
| 池满 | 写满池 | 明确错误，不静默损坏 |
| 切换 backend | 未 unload 切换 | 拒绝或文档化失败 |
| Kylin 4.19 | 全矩阵 | 与 file 模式 SCSI 一致 |

---

## 20. 修订记录

| 版本 | 日期 | 摘要 |
|------|------|------|
| v0.1 | — | 初稿 |
| v0.2 | 2026-06-02 | 拆分 tape_dir/meta/zvol 根；修正快照语义；明确用户态权威创建；meta_dir；命令对照；实施阶段与测试更新 |
