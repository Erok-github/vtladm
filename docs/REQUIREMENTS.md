# VTLADM 需求规格说明书

> 版本：v1.0.0 | 最后更新：2026-06-01
> 适用于功能理解、二次开发和需求验证

---

## 目录

1. [产品概述](#1-产品概述)
2. [用户角色](#2-用户角色)
3. [功能需求](#3-功能需求)
4. [数据模型](#4-数据模型)
5. [业务规则](#5-业务规则)
6. [非功能需求](#6-非功能需求)
7. [系统约束](#7-系统约束)
8. [术语表](#8-术语表)

---

## 1. 产品概述

### 1.1 产品背景

VTLADM（Virtual Tape Library Administration）是一款 Linux 虚拟磁带库管理系统。它通过内核模块 `vtl.ko` 模拟物理磁带库，将普通磁盘文件作为虚拟磁带介质，通过标准 SCSI 接口（`/dev/sg*`、`/dev/st*`）向备份软件（如 Amanda、Bacula、NetBackup 等）暴露标准的磁带机和机械手设备。

### 1.2 产品定位

为备份软件提供无需物理磁带硬件的完整磁带库仿真环境，降低备份基础设施的采购和维护成本，同时保持与现有备份流程的完全兼容。

### 1.3 核心价值

- **零硬件成本**：用磁盘空间替代物理磁带机和磁带介质
- **标准兼容**：通过 SCSI 介质转换器（Medium Changer）和顺序访问设备（Sequential-access）标准命令集仿真
- **多库管理**：支持最多 8 个独立虚拟磁带库，每个库最多 8 个驱动器 + 256 个数据槽位
- **Web 管理**：提供 Vue 3 现代 Web 管理面板，支持完整 CRUD 操作
- **存储网络**：支持 iSCSI 和 FC 导出，可向远程备份服务器暴露虚拟带库
- **压缩仿真**：支持 zlib 和 LZO 硬件压缩模拟

### 1.4 系统架构概览

```
备份软件 (Amanda/Bacula/NetBackup)
           │ SCSI commands
           ▼
┌──────────────────────────┐
│     vtl.ko (内核模块)      │
│  SCSI Medium Changer LUN  │
│  SCSI Sequential-Access   │
│  zlib/LZO 压缩引擎        │
└──────────┬───────────────┘
           │ ioctl / read/write
           ▼
┌──────────────────────────┐
│    vtladm (用户态工具)     │
│  CLI (clap) + Web (axum) │
│  SQLite 目录管理          │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│  磁带镜像文件 (.vtltape)   │
│  侧边元数据 (.vtlmeta)    │
└──────────────────────────┘
```

---

## 2. 用户角色

### 2.1 角色定义

| 角色 | 描述 | 典型场景 |
|---|---|---|
| **系统管理员** | 负责 VTL 系统的安装、配置和维护 | 安装 vtl.ko、创建磁带库、配置 iSCSI 导出 |
| **备份管理员** | 管理磁带介质和备份策略 | 创建磁带、分配槽位、执行巡检、查看容量趋势 |
| **备份软件** | 通过 SCSI 接口自动操作磁带库的软件 | 装载/卸载磁带、读写数据、移动介质 |

### 2.2 权限模型

当前版本为单用户模型：
- 通过 Web 登录（密码 + 算术验证码）进行身份认证
- CLI 命令不需要认证（依赖系统用户权限）
- iSCSI 执行权限需在 Web 中显式开启（安全闸门）

---

## 3. 功能需求

### 3.1 内核模块管理

#### FR-1.1 模块加载与卸载
- 支持通过 `vtl-kernelctl` 脚本管理 vtl.ko 的加载和卸载
- 支持 `insmod` 参数配置几何（驱动器数、槽位数、personality）
- 支持多实例模式（`vtl_instances=2x32,1x10`）

#### FR-1.2 SCSI 设备模拟
- 每个库暴露为 1 个 SCSI host，包含 1 个 Changer LUN + N 个 Tape LUN
- 支持四种 INQUIRY personality：VTL（默认）、IBM 03584L32、STK L700、HP MSL6480
- 支持标准 SCSI 命令集（详见 [SCSI 命令矩阵](#71-scsi-命令矩阵)）

#### FR-1.3 几何管理
- 支持两种内核几何模式：
  - **Legacy 模式**：每次变更需 `SET_INSTANCES` ioctl（可能需 rmmod/insmod）
  - **Fixed 模式（方案 B）**：一次 insmod 满配（8×256×8），日常变更使用 `RESIZE_GEOMETRY` ioctl，无需 rmmod/insmod

#### FR-1.4 压缩仿真
- 支持 zlib 和 LZO 两种压缩算法仿真
- 块格式：16 字节 VTLB 头（magic + uncompressed_size + compressed_size + algorithm）+ 数据
- 压缩不缩小时自动回退到原始存储
- 可通过 MODE SELECT page 0x0F 开关压缩

#### FR-1.5 状态恢复
- 内核模块加载时从 `changer-<instance>.state` 文件恢复机械手状态
- 支持 `robot write-state` 命令持久化当前状态

### 3.2 磁带库管理

#### FR-2.1 创建磁带库
- 在线库：具有 drives × slots 几何配置
- 系统保留 `__offline__` 库（仅货架，无机械手槽位）
- 库名限制：1-64 字符，仅 ASCII 字母数字 + `-` + `_`
- 上限：最多 8 个在线库

#### FR-2.2 删除磁带库
- 仅可删除空库（无磁带）
- 不可删除 `__offline__` 库
- 至少保留一个在线库

#### FR-2.3 查看磁带库
- 列表视图：所有库名称、ID、类型（在线/离线）、创建时间
- 详情视图：完整 inventory（驱动器状态、槽位占用、磁带分布）

### 3.3 磁带管理

#### FR-3.1 创建磁带
- 创建虚拟磁带镜像文件（`.vtltape`），支持稀疏文件
- 附带侧边元数据文件（`.vtlmeta`，24 字节）
- 命名限制：最长 255 字符，不含特殊字符（`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`）
- 支持密度格式：Default LTO, LTO-5 ~ LTO-10
- 支持密度容量范围校验
- 支持压缩配置（算法 + 开关）

#### FR-3.2 批量创建磁带
- 手动批量：提供 `[{name, size, density}]` 列表
- 自动批量：指定数量 + 容量 + 密度，自动命名为 `{库名}_Tape01` 格式
- 全部成功才返回 ok

#### FR-3.3 删除磁带
- 磁带须在货架上且不在驱动器中
- 删除数据库记录和磁盘镜像文件
- 如果镜像文件已被手动删除，仅清理数据库记录（返回 warning）

#### FR-3.4 清空磁带
- 将 `used_bytes` 置 0
- 镜像文件截断为标称容量
- 磁带须在货架上且不在驱动器中

#### FR-3.5 查看磁带
- 列表：名称、条码、容量、已用量、槽位、货架、是否在驱动中
- 分页支持：offset + limit
- 筛选：按库

### 3.4 货架管理

#### FR-4.1 货架概念
- 货架（Shelf）是磁带的"仓库"——未入机械手槽位的磁带存放在货架上
- 每个在线库有默认 `unused` 货架
- `__offline__` 库可管理离线磁带（已取出机械手的磁带）

#### FR-4.2 货架操作
- 创建/删除自定义货架（默认架不可删）
- 查看货架上的磁带列表
- 单盘/批量换架：将磁带从一个货架移到另一个货架
- 批量迁移：将指定货架上的部分/全部磁带迁移到目标货架

### 3.5 机械手操作

#### FR-5.1 槽位管理
- 查看空槽位列表（0-based 编号）
- 单盘入槽：将货架上的磁带分配到指定机械手槽位
- 批量入槽：将多盘磁带批量分配到槽位
- 支持从 `__offline__` 离线库入槽

#### FR-5.2 装载/卸载
- **LOAD**：将槽位中的磁带装入指定驱动器（发送 MOVE_MEDIUM ioctl）
- **UNLOAD**：将驱动器中的磁带卸载回原槽位
- **EJECT**：将槽位中的磁带移到 I/E 导入导出槽（Mailslot）

#### FR-5.3 导入/导出
- **Import**：将外部磁带镜像文件导入指定槽位
- **Export**：将槽位中的磁带导出为镜像文件，可选 SHA256 校验

### 3.6 对账与同步

#### FR-6.1 对账（Reconcile）
- 比较 DB 记录与内核机械手现场状态
- 检测漂移（Drift）：磁带在 DB 中的位置与内核中的实际位置不一致
- 支持 `--pull` 模式：以内核为准更新 DB
- Web 不支持 `--apply`（DB→内核方向），防止误覆盖内核现场状态

#### FR-6.2 自动对齐（Auto-Align）
- 自动处理漂移——离架介质从内核撤出
- 按配置 apply/pull 修复差异

#### FR-6.3 同步（Sync）
- `robot sync`：将 DB 入槽记录同步到内核机械手
- `robot sync-db`：将内核机械手状态写回 DB（供备份软件定时同步）

#### FR-6.4 Inventory
- 从内核获取完整机械手 inventory（所有元素状态）
- 支持 SMC-3 风格的 Primary Volume Tag（三种格式：auto/standard/mtx）

### 3.7 存储网络

#### FR-7.1 iSCSI 导出
- 将虚拟带库通过 iSCSI 暴露给远程备份服务器
- 使用 LIO targetcli 框架
- LUN 布局：LUN0=机械手 sg，LUN1..=各磁带机 sg（pscsi 多 LUN 模式）
- 支持 `--dry-run` 预览模式
- 安全闸门：Web 需显式开启 `allow_iscsi_exec` 才能执行

#### FR-7.2 iSCSI 卸载
- 从 LIO 配置中移除指定库的 iSCSI Target
- 支持 dry-run 预览

#### FR-7.3 FC 配置
- 声明 FC WWPN/WWNN（仅配置记录，不执行 LIO 操作）

#### FR-7.4 传输模式
- 支持三种传输模式：LocalSCSI（本地直连）、iSCSI（网络）、FC（光纤通道）

### 3.8 系统监控

#### FR-8.1 系统资源
- CPU 使用率（百分比 + 核心数）
- 内存使用（总量/已用/百分比）
- 磁盘 I/O（读写字节数，过滤 loop/ram 设备）

#### FR-8.2 容量趋势
- 记录容量快照（按库聚合）
- 查询历史趋势数据点
- 自动清理 90 天前的旧数据

#### FR-8.3 操作日志
- 记录所有变更操作（类别/操作/详情/时间戳）
- 支持按类别筛选
- 自动清理 90 天前的旧日志

### 3.9 巡检

#### FR-9.1 全栈健康检查
- 内核模块状态（vtl.ko 是否加载）
- 设备节点（`/dev/vtl` 是否可访问）
- SCSI 总线（lsscsi 验证 VTL SCSI host 在线）
- 数据库完整性（SQLite 是否响应）
- 几何对齐（DB 拓扑与内核配置是否一致）
- Web 服务可达性

#### FR-9.2 退出码语义
- `0`：全部正常
- `1`：有警告（可恢复问题）
- `2`：有严重问题（需人工干预）

### 3.10 Web 管理面板

#### FR-10.1 登录认证
- 算术验证码（加减乘法）
- 密码 + 验证码双重校验
- 登录频率限制（同 IP 5 秒内最多 3 次）
- 首次登录强制修改密码

#### FR-10.2 仪表盘
- 资产概览（在线库数、驱动器总数、插槽总数、磁带总数）
- 系统性能（CPU/内存/磁盘 IO）
- 容量趋势（进度条 + 历史曲线）
- 操作日志（最近事件）
- 巡检测试结果（ok/warn/crit 标签）

#### FR-10.3 磁带库管理页面
- 库列表 + 创建/删除
- 库详情（6 Tab：概览、货架、批量建带、磁带入槽、传输配置、对账）
- 新建库时显示 SCSI LUN 总数预览

#### FR-10.4 账户安全
- 修改密码（新密码 ≥ 8 字符，两次输入一致，修改后自动登出）
- 管理活跃会话（查看、撤销非当前会话）

#### FR-10.5 首次设置向导
- 3 步向导：基础路径 → 内核模块配置 → 管理密码
- 内核模块配置：vtl.ko 路径、重载脚本、INQUIRY 厂商选择

### 3.11 CLI 命令行

完整的 CLI 命令集，支持脚本化和自动化：
- 磁带 CRUD：`create-tape`, `delete-tape`, `init-tape`, `list-tapes`
- 批量操作：`batch-create`, `batch-import`
- 机械手操作：`load`, `unload`, `eject`, `assign-slot`
- 库管理：`library create/delete/list`
- 货架管理：`shelf create/delete/list/tapes/place`
- 对账：`robot reconcile/sync/auto-align/sync-db/write-state`
- 巡检：`patrol`
- 传输：`transport show/check/guide`
- 搜索：`search`（按名称、标签、容量范围、空闲空间）
- 配置：`config set/show`, `init-config`
- Web 服务：`serve`, `reset-web-auth`

---

## 4. 数据模型

### 4.1 实体关系图

```
vtl_libraries (1) ──< (N) shelves
      │
      ├──< (N) tapes ──< (N) tape_tags >── (N) tags
      │
      ├──< (N) slots
      │
      ├──< (N) drives
      │
      └──< (N) library_config
```

### 4.2 表结构

#### vtl_libraries（磁带库）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | INTEGER | PK, AUTOINCREMENT | 库 ID |
| name | TEXT | NOT NULL, UNIQUE | 库名称 |
| created_at | TEXT | NOT NULL | 创建时间（ISO 格式） |

#### shelves（货架）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | INTEGER | PK, AUTOINCREMENT | 货架 ID |
| library_id | INTEGER | FK→vtl_libraries.id, NOT NULL | 所属库 |
| name | TEXT | NOT NULL | 货架名称 |
| is_default_unused | INTEGER | NOT NULL, DEFAULT 0 | 是否默认 unused 架 |
| _约束_ | UNIQUE | (library_id, name) | 库内货架名唯一 |

#### tapes（磁带）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | INTEGER | PK, AUTOINCREMENT | 磁带 ID |
| library_id | INTEGER | FK→vtl_libraries.id, NOT NULL | 所属库 |
| shelf_id | INTEGER | FK→shelves.id | 所在货架 |
| barcode | TEXT | NOT NULL | 条码（格式 VTLXXXXXX） |
| name | TEXT | NOT NULL | 磁带名称 |
| slot | INTEGER | — | 机械手槽位（NULL=在货架上） |
| capacity_bytes | INTEGER | NOT NULL | 标称容量（字节） |
| used_bytes | INTEGER | DEFAULT 0 | 已用容量（字节） |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |
| image_path | TEXT | NOT NULL | 镜像文件路径 |
| density_code | INTEGER | NOT NULL, DEFAULT 40 | T10 密度码 |
| _约束_ | UNIQUE | (library_id, name) | 库内磁带名唯一 |
| _约束_ | UNIQUE | (library_id, barcode) | 库内条码唯一 |

#### slots（槽位）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| library_id | INTEGER | FK→vtl_libraries.id, NOT NULL | 所属库 |
| slot_id | INTEGER | NOT NULL | 槽位编号（0-based） |
| tape_id | INTEGER | FK→tapes.id | 占用磁带的 ID |
| is_import_export | INTEGER | DEFAULT 0 | 是否 I/E 槽 |
| _约束_ | PK | (library_id, slot_id) | 复合主键 |

#### drives（驱动器）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| library_id | INTEGER | FK→vtl_libraries.id, NOT NULL | 所属库 |
| drive_id | INTEGER | NOT NULL | 驱动器编号（0-based） |
| tape_id | INTEGER | FK→tapes.id | 装载磁带的 ID |
| _约束_ | PK | (library_id, drive_id) | 复合主键 |

#### library_config（库级配置）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| library_id | INTEGER | FK→vtl_libraries.id, NOT NULL | 所属库 |
| key | TEXT | NOT NULL | 配置键 |
| value | TEXT | — | 配置值 |
| _约束_ | PK | (library_id, key) | 复合主键 |

#### config（全局配置）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| key | TEXT | PK | 配置键 |
| value | TEXT | — | 配置值 |

#### tags（标签）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | INTEGER | PK | 标签 ID |
| name | TEXT | UNIQUE, NOT NULL | 标签名称 |

#### tape_tags（磁带-标签关联）

| 列 | 类型 | 约束 | 说明 |
|---|---|---|---|
| tape_id | INTEGER | FK→tapes.id | 磁带 ID |
| tag_id | INTEGER | FK→tags.id | 标签 ID |
| _约束_ | PK | (tape_id, tag_id) | 复合主键 |

#### iscsi_library_exports（iSCSI 导出记录）

| 列 | 类型 | 说明 |
|---|---|---|
| iqn | TEXT | iSCSI 限定名称 |
| export_id | TEXT | 导出标识符 |
| library | TEXT | 库名称 |
| changer_sg | TEXT | 机械手 SCSI 通用设备路径 |
| drive_sg | TEXT | 磁带机 SCSI 通用设备路径 |
| lun_map | TEXT | LUN 映射（可选） |
| portal_ip | TEXT | iSCSI 门户 IP |
| portal_port | INTEGER | iSCSI 门户端口 |
| exported_at | TEXT | 导出时间戳 |

#### vtl_capacity_log（容量快照）

| 列 | 类型 | 说明 |
|---|---|---|
| ts | TEXT | 快照时间 |
| library | TEXT | 库名称 |
| total_bytes | INTEGER | 总容量 |
| used_bytes | INTEGER | 已用容量 |
| tape_count | INTEGER | 磁带数量 |

#### operation_events（操作事件日志）

| 列 | 类型 | 说明 |
|---|---|---|
| id | INTEGER PK | 自增 ID |
| ts | TEXT | 事件时间 |
| category | TEXT | 事件类别 |
| action | TEXT | 操作动作 |
| detail | TEXT | 详细信息 |

---

## 5. 业务规则

### 5.1 磁带生命周期

```
创建(Create) → 在货架上 → 入槽(Assign Slot) → 在机械手槽位中
                                                    │
                                         装载(Load) │
                                                    ▼
                                          在驱动器中(In Drive)
                                                    │
                                        卸载(Unload) │
                                                    ▼
                                          在机械手槽位中
                                                    │
                                   弹出(Eject) / 回架(Place)
                                                    │
                                          在货架上 / I/E槽
                                                    │
                                         删除(Delete) / 清空(Init)
                                                    ▼
                                                  已销毁
```

### 5.2 操作约束

| 操作 | 前提条件 |
|---|---|
| 删除磁带 | 在货架上 + 不在驱动器中 |
| 清空磁带 | 在货架上 + 不在驱动器中 |
| 入槽 | 在货架上 + 目标槽位为空 |
| 装载 | 源槽位有磁带 + 目标驱动器为空 |
| 卸载 | 驱动器中有磁带 |
| 弹出 | 槽位有磁带 + I/E 槽为空 |
| 删除货架 | 架上无磁带 + 非默认 unused 架 |
| 删除库 | 库内无磁带 + 非 `__offline__` + 至少保留一个在线库 |

### 5.3 槽位编号约定

| 元素类型 | 编号方案 |
|---|---|
| 数据槽位 (Data Slots) | **0-based**（内部） |
| 驱动器 (Drives) | 1000 + drive_index（SCSI 元素地址） |
| I/E 槽 (Mailslots) | 2000 + mail_index（SCSI 元素地址） |

### 5.4 密度容量校验

| 密度格式 | 最小容量 | 最大容量 |
|---|---|---|
| Default LTO | 100 MB | 1 TB |
| LTO-5 | 1 GB | 3 TB |
| LTO-6 | 1 GB | 6 TB |
| LTO-7 | 1 GB | 15 TB |
| LTO-8 | 1 GB | 30 TB |
| LTO-9 | 1 GB | 45 TB |
| LTO-10 | 1 GB | 90 TB |

### 5.5 产品上限

| 限制 | 值 | 说明 |
|---|---|---|
| 最大在线库数 | 8 | 与内核 `VTL_MAX_SCSI_INSTANCES` 一致 |
| 每库最大驱动器数 | 8 | 与内核 `VTL_MAX_DRIVES` 一致 |
| 每库最大数据槽位数 | 256 | 与内核 `VTL_MAX_SLOTS` 一致 |
| I/E 槽数 | 4 | 固定值（内核 `VTL_MAX_MAILSLOTS`） |
| 磁带名最大长度 | 255 字符 | 文件系统限制 |
| 库名最大长度 | 64 字符 | 仅 ASCII 字母数字 + `-` + `_` |

### 5.6 内核真相原则

- 内核机械手状态是**唯一的现场真相**
- DB 中的 `tapes.slot` 是**目录提示**，不表示现场实际位置
- 对账时内核→DB 方向（pull）安全，DB→内核方向（apply）在 Web 中禁用
- 备份软件通过 SCSI 直接操作内核机械手，用户态通过 ioctl 操作内核机械手

### 5.7 密码安全

| 规则 | 说明 |
|---|---|
| 最小长度 | 8 字符 |
| 存储方式 | bcrypt 哈希 |
| 登录限制 | 同一 IP 5 秒内最多 3 次尝试 |
| 锁定时长 | 5 分钟（连续 5 次失败后） |
| 会话有效期 | 24 小时（绝对）/ 30 分钟（空闲） |
| Cookie 属性 | HttpOnly, SameSite=Lax |

---

## 6. 非功能需求

### 6.1 性能

| 指标 | 目标 |
|---|---|
| Web API 响应时间 | < 200ms（正常负载） |
| 磁带创建 | < 1s（含稀疏文件分配） |
| 数据库 busy_timeout | 8 秒 |
| lsscsi 扫描 | < 2s |
| 巡检执行 | < 5s |

### 6.2 可靠性

- 内核 panic 防护：rmmod 前检查设备持有者和 LIO 引用
- 并发安全：SQLite busy_timeout + 线程本地库名
- 日志轮转：单文件上限后可轮转为 `*.1` ~ `*.5`
- 状态恢复：内核模块加载时从持久化文件恢复机械手状态
- 库守卫（LibraryGuard）：RAII 模式确保线程本地库名及时恢复

### 6.3 安全性

- CSRF 防护：所有写操作需携带 CSRF Token
- Session 安全：HttpOnly Cookie + 空闲超时自动失效
- 页面关闭即登出：beforeunload sendBeacon 到 /api/logout
- 密码强度：最低 8 字符 + bcrypt 哈希
- 验证码：算术验证码防暴力破解
- iSCSI 执行闸门：Web 需显式开启 allow_iscsi_exec
- 数据库文件权限：0600（仅所有者可读写）

### 6.4 兼容性

| 组件 | 要求 |
|---|---|
| Linux 内核 | 4.18 – 6.10 |
| 架构 | x86_64 (64-bit only) |
| Rust 工具链 | ≥ 1.66 |
| GCC | 系统默认 |
| 浏览器（Web UI） | 现代浏览器（Chrome/Firefox/Edge 最新版） |

### 6.5 可维护性

- 前端使用 TypeScript + Naive UI 组件库，组件化开发
- 后端使用 Rust + Axum，类型安全
- 约 80+ 自动化测试用例覆盖核心功能
- 代码文件大小控制在 2000 行以内（web.rs 除外，为历史遗留）
- 日志记录所有关键操作（带时间戳和类别）

---

## 7. 系统约束

### 7.1 SCSI 命令矩阵

#### Changer LUN (LUN 0)

| 命令 | 支持 |
|---|---|
| INQUIRY | 是 |
| TEST UNIT READY | 是 |
| REQUEST SENSE | 是 |
| MODE SENSE (6)/(10) | 是 |
| MODE SELECT (6)/(10) | 是 |
| INITIALIZE ELEMENT STATUS | 是 |
| MOVE MEDIUM | 是 |
| READ ELEMENT STATUS | 是 |
| PREVENT ALLOW MEDIUM REMOVAL | 是 |
| REPORT LUNS | 是 |

#### Tape LUN (LUN 1..N)

| 命令 | 支持 |
|---|---|
| INQUIRY | 是 |
| TEST UNIT READY | 是 |
| REQUEST SENSE | 是 |
| READ BLOCK LIMITS | 是 |
| MODE SENSE (6)/(10) | 是 |
| MODE SELECT (6)/(10) | 是 |
| READ (6)/(10)/(12) | 是 |
| WRITE (6)/(10)/(12) | 是 |
| REWIND | 是 |
| SPACE | 是 |
| WRITE FILEMARKS | 是 |
| LOAD/UNLOAD | 是 |
| LOG SENSE | 是 (0x00, 0x02, 0x03, 0x06, 0x0C, 0x2E, 0x11) |
| READ POSITION | 是 |
| PREVENT ALLOW MEDIUM REMOVAL | 是 |
| REPORT DENSITY SUPPORT | 是 |
| READ CAPACITY (10)/(16) | 是 |

### 7.2 文件布局

```
/opt/vtladm/                    # 安装根目录
├── bin/vtladm                  # 主管理程序
├── sbin/vtl-kernelctl          # 内核模块控制脚本
├── ko/vtl.ko                   # 内核模块
├── var/
│   ├── vtl.conf                # 配置文件
│   ├── vtl.db                  # SQLite 数据库
│   ├── tapes/
│   │   └── <库名>/
│   │       ├── <磁带名>.vtltape  # 磁带镜像文件
│   │       └── <磁带名>.vtlmeta  # 侧边元数据
│   └── log/
│       ├── vtladm.log          # 操作日志
│       └── web_admin.json      # Web 认证文件
└── systemd/                    # systemd 单元文件
```

---

## 8. 术语表

| 术语 | 英文 | 说明 |
|---|---|---|
| 虚拟磁带库 | Virtual Tape Library (VTL) | 用磁盘文件模拟的磁带库系统 |
| 磁带库 | Library | 包含机械手、驱动器、槽位的完整带库实例 |
| 机械手 | Changer / Robot | 在槽位与驱动器之间搬运磁带的自动化装置 |
| 驱动器 | Drive | 读写磁带的设备，同一时刻只能装载一盘磁带 |
| 槽位 | Slot | 机械手中存放磁带的固定位置 |
| 数据槽位 | Data Slot | 用于存放数据磁带的常规槽位 |
| I/E 槽 | Mailslot / Import-Export Slot | 导入导出槽，用于向机械手装入/取出磁带 |
| 货架 | Shelf | 磁带"仓库"——未入机械手槽位的磁带的存放处 |
| 磁带 | Tape | 备份数据的虚拟载体，对应磁盘上的 `.vtltape` 文件 |
| 条码 | Barcode | 磁带唯一标识，格式 VTLXXXXXX |
| 密度 | Density | T10 SCSI 标准磁带密度码（0x40-0x58） |
| 入槽 | Assign Slot | 将货架上的磁带放入机械手槽位 |
| 装载 | Load / Mount | 将磁带从槽位移入驱动器 |
| 卸载 | Unload | 将磁带从驱动器移回槽位 |
| 换架 | Shelf Place | 将磁带从一个货架移到另一个货架 |
| 对账 | Reconcile | 比较 DB 记录与内核现场状态的差异 |
| 漂移 | Drift | DB 与内核之间的磁带位置不一致 |
| 巡检 | Patrol | 全栈健康检查 |
| 离线库 | Offline Library | `__offline__` 保留库，管理已从机械手取出的磁带 |
| 侧边元数据 | Sidecar Metadata | `.vtlmeta` 文件，存储磁带元信息（密度、压缩标志等） |
| 方案 B | Plan B / Fixed Mode | 一次 insmod 满配 + resize ioctl 的内核几何模式 |
| PVolTag | Primary Volume Tag | SMC-3 标准的卷标信息，随 READ ELEMENT STATUS 返回 |
| Personality | — | INQUIRY 响应的厂商模拟身份（VTL/IBM/STK/HP） |
