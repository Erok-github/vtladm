# VTLADM API 接口文档

> 版本：v1.0.0 | 最后更新：2026-06-01
> 适用于二次开发、前端对接和自动化脚本集成

---

## 目录

1. [概述](#1-概述)
2. [认证与会话](#2-认证与会话)
3. [磁带库管理](#3-磁带库管理)
4. [磁带管理](#4-磁带管理)
5. [货架管理](#5-货架管理)
6. [槽位与入槽](#6-槽位与入槽)
7. [机械手操作](#7-机械手操作)
8. [机械手对账](#8-机械手对账)
9. [iSCSI 传输](#9-iscsi-传输)
10. [SCSI 设备扫描](#10-scsi-设备扫描)
11. [系统监控](#11-系统监控)
12. [巡检](#12-巡检)
13. [传输架构](#13-传输架构)
14. [会话管理](#14-会话管理)
15. [初始设置](#15-初始设置)
16. [全局约定](#16-全局约定)

---

## 1. 概述

### 1.1 基本信息

| 属性 | 值 |
|---|---|
| 协议 | HTTP/1.1 |
| 数据格式 | JSON |
| 字符编码 | UTF-8 |
| 服务端口 | `8765`（默认） |
| 监听地址 | `127.0.0.1`（默认，仅本机） |

### 1.2 路由分组

所有 API 路径以 `/api/` 为前缀，按功能分为以下几组：

| 分组 | 路径前缀 | 说明 |
|---|---|---|
| 认证 | `/api/` | 登录、登出、验证码、修改密码 |
| 设置 | `/api/setup/` | 首次安装向导 |
| 会话 | `/api/session/`, `/api/sessions/` | 会话管理 |
| 磁带库 | `/api/libraries`, `/api/library/` | 库的查询 |
| 磁带 | `/api/tapes`, `/api/manage/tape/` | 磁带的 CRUD |
| 货架 | `/api/shelves`, `/api/manage/shelf/` | 货架管理 |
| 机械手 | `/api/manage/robot/` | 同步、对账、自动对齐 |
| 传输 | `/api/manage/transport/` | SCSI 设备扫描 |
| iSCSI | `/api/manage/iscsi/` | iSCSI 导出管理 |
| 监控 | `/api/monitor/` | 系统资源、容量趋势、事件日志 |
| 巡检 | `/api/patrol` | 全栈健康检查 |

### 1.3 通用响应格式

**成功响应：**
```json
{ "ok": true, "data": { ... } }
```

**错误响应：**
```json
{ "error": "错误描述信息" }
```
HTTP 状态码：`400`（参数错误）、`401`（未认证）、`403`（禁止）、`500`（服务器错误）

### 1.4 认证机制

- 基于 **HttpOnly Cookie** 的会话管理
- Cookie 名：`vtl_session`（会话令牌）、`vtl_csrf`（CSRF Token）
- 所有非公开 API 需要有效会话
- 非 GET 请求必须在 Header 中携带 CSRF Token：`X-VTL-CSRF: <token>`
- 401 响应表示会话过期或未登录

---

## 2. 认证与会话

### 2.1 GET /api/captcha — 获取登录验证码

**认证**：无需

**响应：**
```json
{
  "captcha_id": "abc123",
  "question": "3 + 7 = ?"
}
```

**说明**：算术验证码，服务端生成 captcha_id 和算术题。每次调用生成新验证码。

---

### 2.2 POST /api/login — 登录

**认证**：无需

**请求体：**
```json
{
  "username": "admin",
  "password": "your_password",
  "captcha_id": "abc123",
  "captcha_answer": "10"
}
```

**成功响应 (200)：**
```json
{
  "ok": true,
  "must_change_password": false
}
```

**说明**：
- 登录成功设置 `vtl_session` 和 `vtl_csrf` Cookie
- 首次登录或密码重置后 `must_change_password` 为 `true`
- 登录频率限制：同一 IP+UA 组合 5 秒内最多 3 次尝试
- Cookie 有效期：24 小时，Secure 属性取决于是否为 HTTPS

---

### 2.3 POST /api/logout — 登出

**认证**：需要

**请求体**：无（空 JSON `{}` 也可）

**成功响应 (200)：**
```json
{ "ok": true }
```

**说明**：清除服务端会话和客户端 Cookie。`/api/logout` 豁免 CSRF 检查，确保 `beforeunload sendBeacon` 能正常注销。

---

### 2.4 POST /api/change-password — 修改密码

**认证**：需要

**请求体：**
```json
{
  "old_password": "current_password",
  "new_password": "new_password_8_chars_min"
}
```

**成功响应 (200)：**
```json
{ "ok": true }
```

**校验规则**：
- 新密码至少 8 个字符
- 旧密码必须正确
- 密码使用 bcrypt 哈希存储

---

### 2.5 GET /api/session/ping — 检查会话有效性

**认证**：无需（但携带 Cookie 时会验证）

**成功响应 (200)：**
```json
{ "ok": true }
```

**说明**：前端路由守卫调用此接口判断用户是否已登录。未登录返回 401。

---

## 3. 磁带库管理

### 3.1 GET /api/libraries — 获取所有磁带库列表

**认证**：需要

**查询参数**：无

**响应：**
```json
{
  "libraries": [
    {
      "id": 1,
      "name": "marstor",
      "created_at": "2025-01-15T10:30:00",
      "is_offline_storage": false
    }
  ],
  "db_path": "/opt/vtladm/var/vtl.db",
  "online_count": 1,
  "vtl_scsi_lines": 2,
  "hint": null,
  "product_limits": {
    "max_online_libraries": 8,
    "max_drives_per_library": 8,
    "max_data_slots_per_library": 256
  }
}
```

**字段说明**：

| 字段 | 类型 | 说明 |
|---|---|---|
| `libraries[].id` | int | 库 ID |
| `libraries[].name` | string | 库名称 |
| `libraries[].created_at` | string | 创建时间 |
| `libraries[].is_offline_storage` | bool | 是否为离线保管库（`__offline__`） |
| `db_path` | string | SQLite 数据库路径 |
| `online_count` | int | 在线库数量 |
| `vtl_scsi_lines` | int | lsscsi 检测到的 VTL SCSI 设备数 |
| `hint` | string\|null | 无在线库时的提示信息 |
| `product_limits` | object | 产品上限 |

---

### 3.2 GET /api/libraries-status — 获取所有库状态摘要

**认证**：需要

**响应：**
```json
{
  "libraries": [
    {
      "library": "marstor",
      "tape_count": 50,
      "loaded_in_drives": 2,
      "drives": 4,
      "data_slots": 32
    }
  ]
}
```

---

### 3.3 GET /api/library/detail — 获取库详情

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |

**响应：** 见 [附录 A：LibraryDetailResponse](#附录-a-librarydetailresponse)

**说明**：这是最全面的库信息接口，包含库元数据、驱动器状态、磁带列表、机械手 inventory（data_slots / drives / mailslots 的元素占用情况）。单次调用即可获取库详情页所需全部数据。

---

### 3.4 POST /api/manage/library/create — 创建磁带库

**认证**：需要

**请求体：**
```json
{
  "name": "marstor",
  "drives": 4,
  "slots": 32
}
```

**校验规则**：
- `name`：1-64 字符，仅允许 ASCII 字母、数字、`-`、`_`
- `drives`：1-8
- `slots`：1-256
- 在线库总数不超过 8

**成功响应 (200)：**
```json
{
  "ok": true,
  "kernel_geom": "4x32",
  "kernel_geom_detail": "drives=4 slots=32",
  "scsi_rescan": "rescan triggered for host5"
}
```

---

### 3.5 POST /api/manage/library/delete — 删除磁带库

**认证**：需要

**请求体：**
```json
{
  "name": "marstor"
}
```

**校验规则**：
- 不可删除 `__offline__`
- 至少保留一个在线库
- 库内须无磁带

**成功响应 (200)：**
```json
{
  "ok": true,
  "file_warnings": null,
  "kernel_geom": "removed",
  "kernel_geom_detail": "library marstor removed"
}
```

---

## 4. 磁带管理

### 4.1 GET /api/tapes — 获取磁带列表

**认证**：需要

**查询参数：**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `library` | string | 当前库 | 库名称 |
| `offset` | int | 0 | 分页偏移量 |
| `limit` | int | 5000 | 每页条数（最大 50000） |

**响应：**
```json
{
  "library": "marstor",
  "tapes": [
    {
      "name": "TAPE001",
      "barcode": "VTL00A1B2",
      "capacity_bytes": 107374182400,
      "used_bytes": 0,
      "slot": 5,
      "shelf_name": "unused",
      "in_drive": false
    }
  ],
  "total": 50,
  "offset": 0,
  "limit": 5000,
  "truncated": false
}
```

**说明**：
- `slot`: `null` 表示未入槽（在货架上），数字为 0-based 槽位编号
- `shelf_name`: `null` 表示不在货架上（仅在机械手槽内或驱动中）
- `in_drive`: 磁带当前是否在驱动器中

---

### 4.2 GET /api/manage/tape/density-limits — 获取密度容量范围

**认证**：需要

**响应：**
```json
{
  "density_limits": [
    {
      "code": "0x40",
      "label": "Default LTO",
      "min_bytes": 104857600,
      "max_bytes": 1099511627776,
      "min_human": "100M",
      "max_human": "1T"
    },
    {
      "code": "0x4A", "label": "LTO-5",
      "min_bytes": 1073741824, "max_bytes": 3298534883328,
      "min_human": "1G", "max_human": "3T"
    },
    {
      "code": "0x4C", "label": "LTO-6",
      "min_bytes": 1073741824, "max_bytes": 6597069766656,
      "min_human": "1G", "max_human": "6T"
    },
    {
      "code": "0x4E", "label": "LTO-7",
      "min_bytes": 1073741824, "max_bytes": 16492674416640,
      "min_human": "1G", "max_human": "15T"
    },
    {
      "code": "0x50", "label": "LTO-8",
      "min_bytes": 1073741824, "max_bytes": 32985348833280,
      "min_human": "1G", "max_human": "30T"
    },
    {
      "code": "0x52", "label": "LTO-9",
      "min_bytes": 1073741824, "max_bytes": 49478023249920,
      "min_human": "1G", "max_human": "45T"
    },
    {
      "code": "0x58", "label": "LTO-10",
      "min_bytes": 1073741824, "max_bytes": 98956046499840,
      "min_human": "1G", "max_human": "90T"
    }
  ]
}
```

---

### 4.3 POST /api/manage/tape/create — 创建单个磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "name": "TAPE001",
  "size": "100G",
  "shelf": "unused",
  "density": "LTO-8"
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名 |
| `name` | string | 是 | 磁带名称（最长 255 字符，不含 `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`） |
| `size` | string | 是 | 容量，支持 B/K/M/G/T 单位（如 `100G`、`1T`） |
| `shelf` | string | 否 | 目标货架名，默认 `"unused"` |
| `density` | string | 否 | 密度格式，默认 `"Default LTO"`，支持 `LTO-5`~`LTO-10`、`0x40` 等十六进制 |

**校验**：
- 磁带名在库内唯一（全局也需唯一，防止内核 `filp_open` 冲突）
- 容量必须在对应密度格式的允许范围内
- 不超过配额限制

**成功响应 (200)：**
```json
{ "ok": true }
```

---

### 4.4 POST /api/manage/tape/create-batch — 批量创建磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "shelf": "unused",
  "items": [
    { "name": "TAPE001", "size": "100G", "density": "LTO-8" },
    { "name": "TAPE002", "size": "200G", "density": "LTO-9" }
  ]
}
```

**说明**：每项 `density` 可选。全部成功才返回 `ok`。

---

### 4.5 POST /api/manage/tape/create-auto-batch — 批量自动建带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "count": 10,
  "size": "100G",
  "shelf": "unused",
  "density": "LTO-8"
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `count` | int | 是 | 创建数量（1-100） |
| `size` | string | 是 | 容量 |
| `shelf` | string | 否 | 目标货架 |
| `density` | string | 否 | 密度格式 |

**自动命名规则**：`{库名}_Tape01`, `{库名}_Tape02`, ...

**成功响应 (200)：**
```json
{
  "ok": true,
  "names": ["marstor_Tape01", "marstor_Tape02", "..."],
  "count": 10
}
```

---

### 4.6 POST /api/manage/tape/delete — 删除磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "name": "TAPE001"
}
```

**校验**：磁带须在货架上且不在驱动器中。

**成功响应 (200)：**
```json
{ "ok": true }
```
或带警告：
```json
{ "ok": true, "warning": "磁带文件已不存在，仅删除数据库记录" }
```

---

### 4.7 POST /api/manage/tape/init — 清空磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "name": "TAPE001"
}
```

**说明**：将 `used_bytes` 置 0，镜像文件截断为标称容量。磁带须在货架上且不在驱动器中。

**成功响应 (200)：**
```json
{ "ok": true }
```

---

### 4.8 POST /api/manage/tape/shelf-place — 单盘换架/回架

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "tape": "TAPE001",
  "shelf": "offsite_archive"
}
```

**说明**：将磁带从当前货架移到目标货架。`shelf` 可选，不提供则移到默认 "unused" 架。

---

### 4.9 POST /api/manage/tape/shelf-place-batch — 批量换架

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "tapes": ["TAPE001", "TAPE002", "TAPE003"],
  "shelf": "offsite_archive"
}
```

---

### 4.10 POST /api/manage/tape/migrate-shelves-batch — 批量迁移货架

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "from_shelf": "old_shelf",
  "to_shelf": "new_shelf",
  "tapes": ["TAPE001", "TAPE002"]
}
```

**说明**：将指定磁带从 `from_shelf` 迁移到 `to_shelf`。不传 `tapes` 则迁移该架上所有磁带。

---

## 5. 货架管理

### 5.1 GET /api/shelves — 获取货架列表

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |

**响应：**
```json
{
  "library": "marstor",
  "shelves": [
    { "id": 1, "name": "unused", "is_default_unused": true },
    { "id": 2, "name": "offsite_archive", "is_default_unused": false }
  ]
}
```

---

### 5.2 GET /api/offline-shelves — 获取离线库货架

**认证**：需要

**响应**：格式同 `/api/shelves`，但 library 固定为 `__offline__`。

---

### 5.3 POST /api/manage/shelf/create — 创建货架

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "name": "offsite_archive"
}
```

---

### 5.4 POST /api/manage/shelf/delete — 删除货架

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "name": "offsite_archive"
}
```

**校验**：架上须无磁带；默认 "unused" 架不可删。

---

### 5.5 POST /api/manage/shelf/create-offline — 创建离线货架

**认证**：需要

**请求体：**
```json
{
  "name": "vault_2025"
}
```

**说明**：在 `__offline__` 库下创建货架，用于管理已离线的磁带。

---

## 6. 槽位与入槽

### 6.1 GET /api/empty-slots — 获取空槽位列表

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |

**响应：**
```json
{
  "library": "marstor",
  "empty_slots": [0, 3, 7, 12],
  "empty_slot_count": 4
}
```

**说明**：槽位编号为 **0-based**。

---

### 6.2 POST /api/manage/tape/assign-slot — 单盘入槽

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "tape": "TAPE001",
  "slot": 5,
  "from_offline": false
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `from_offline` | bool | 否 | 磁带是否从 `__offline__` 离线库入槽 |

---

### 6.3 POST /api/manage/tape/assign-slot-batch — 批量入槽

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "pairs": [
    { "tape": "TAPE001", "slot": 0, "from_offline": false },
    { "tape": "TAPE002", "slot": 1, "from_offline": true }
  ]
}
```

---

## 7. 机械手操作

### 7.1 POST /api/manage/tape/load — 装载磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "slot": 5,
  "drive": 0
}
```

**说明**：将槽位中的磁带装入指定驱动器（通过 ioctl 通知内核机械手执行 MOVE_MEDIUM）。

---

### 7.2 POST /api/manage/tape/unload — 卸载磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "drive": 0
}
```

**成功响应 (200)：**
```json
{
  "ok": true,
  "slot": 5
}
```

`slot` 为卸载后磁带所在的槽位编号。

---

### 7.3 POST /api/manage/tape/eject — 弹出磁带

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "slot": 3
}
```

**成功响应 (200)：**
```json
{
  "ok": true,
  "mailslot": 0
}
```

**说明**：将槽位中的磁带移到 I/E 导入导出槽（Mailslot）。

---

## 8. 机械手对账

### 8.1 POST /api/manage/robot/reconcile — 对账

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "pull": true
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `pull` | bool | 否 | `true` 以内核为准更新 DB；省略则仅比较差异 |

**注意**：Web 不支持 `--apply`（DB→内核方向），防止误覆盖内核现场状态。

**响应：**
```json
{
  "ok": true,
  "drift_count": 3,
  "fixes_applied": 0,
  "pull_updates": 3,
  "inventory_truncated": false,
  "drifts": [
    { "tape": "TAPE001", "db": "slot 5", "kernel": "drive 0" },
    { "tape": "TAPE002", "db": null, "kernel": "slot 3" }
  ]
}
```

---

### 8.2 POST /api/manage/robot/auto-align — 自动对齐

**认证**：需要

**请求体：**
```json
{
  "library": "marstor"
}
```

**说明**：自动处理漂移 — 离架介质从内核撤出，按配置 apply/pull 修复差异。

**响应：**
```json
{
  "ok": true,
  "evacuated": 1,
  "fixes_applied": 2,
  "pull_updates": 0,
  "drifts_remaining": 0
}
```

---

### 8.3 POST /api/manage/robot/sync — 同步 DB 到内核

**认证**：需要

**请求体：**
```json
{
  "library": "marstor"
}
```

**响应：**
```json
{
  "ok": true,
  "tapes_updated": 3
}
```

---

## 9. iSCSI 传输

### 9.1 GET /api/manage/iscsi/config — 获取 iSCSI 配置

**认证**：需要

**响应：**
```json
{
  "tape_dir": "/opt/vtladm/var/tapes",
  "transport": "iscsi",
  "iscsi_iqn": "iqn.2025-01.com.example:vtl.marstor",
  "iscsi_portals": "192.168.1.100:3260",
  "portal_ip_suggested": "192.168.1.100",
  "portal_port_suggested": 3260,
  "vtladm_iscsi_path": "/opt/vtladm/bin/vtladm-iscsi",
  "allow_iscsi_exec": false,
  "non_unix_build": false,
  "kernel_reload_on_db_change": false,
  "kernel_geom_prefer_ioctl": true
}
```

---

### 9.2 POST /api/manage/iscsi/allow-exec — 设置 iSCSI 执行权限

**认证**：需要

**请求体：**
```json
{
  "allow": true
}
```

**说明**：设为 `true` 后，Web 界面才可实际执行 `vtladm-iscsi` 命令（导出/卸载）。安全闸门，防止误操作。

**响应：**
```json
{
  "ok": true,
  "allow_iscsi_exec": true
}
```

---

### 9.3 POST /api/manage/iscsi/check — 检查 iSCSI 环境

**认证**：需要

**请求体：**
```json
{
  "sudo": false
}
```

**响应：**
```json
{
  "ok": true,
  "stdout": "targetcli is available\n...",
  "stderr": "",
  "dry_run": true
}
```

---

### 9.4 GET /api/manage/iscsi/library-export-defaults — 获取导出默认值

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |
| `regenerate` | int | 否 | 设为 `1` 忽略已有记录生成新 IQN |

**响应：**
```json
{
  "library": "marstor",
  "iqn": "iqn.2025-01.com.example:vtl.marstor",
  "export_id": "marstor",
  "backend_ch": "marstor_lib",
  "backend_drives": ["marstor_drive0", "marstor_drive1"],
  "portal_ip": "192.168.1.100",
  "portal_port": 3260,
  "drive_count": 2,
  "default_lun_map": [0, 1, 2],
  "changer_sg": "sg3",
  "drive_sg": ["sg4", "sg5"],
  "has_saved_export": false,
  "exported_at": null,
  "saved_drive_mismatch": false,
  "can_export": true,
  "export_blocked_reason": null,
  "product_limits": { "max_online_libraries": 8, ... }
}
```

---

### 9.5 POST /api/manage/iscsi/library-export — 导出库到 iSCSI

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "iqn": "iqn.2025-01.com.example:vtl.marstor",
  "export_id": "marstor",
  "changer_sg": "sg3",
  "drive_sg": ["sg4", "sg5"],
  "lun_map": [0, 1, 2],
  "portal_ip": "192.168.1.100",
  "portal_port": 3260,
  "dry_run": true,
  "sudo": false
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `dry_run` | bool | 否 | `true` 仅预览不执行 |
| `sudo` | bool | 否 | 是否通过 sudo 执行 |

**说明**：实际执行需要 `allow_iscsi_exec=true`。使用 pscsi 多 LUN 模式，LUN0=机械手，LUN1..=磁带机。

---

### 9.6 POST /api/manage/iscsi/library-unexport — 卸载 iSCSI 导出

**认证**：需要

**请求体：** 同 export，`dry_run` 控制是否实际执行。

---

### 9.7 POST /api/manage/iscsi/quick-export — 快速导出

**认证**：需要

**请求体：**
```json
{
  "library": "marstor",
  "portal_ip": "192.168.1.100",
  "portal_port": 3260,
  "sudo": false
}
```

---

### 9.8 POST /api/manage/iscsi/quick-unexport — 快速卸载

**认证**：需要

**请求体：**
```json
{
  "library": "marstor"
}
```

---

## 10. SCSI 设备扫描

### 10.1 GET /api/manage/transport/scan-sg — 扫描 SCSI 设备

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |
| `transport` | string | 否 | 传输提示：`local`/`iscsi`/`fc` |
| `prefer_scsi_host` | int | 否 | 优先选用的 SCSI host 编号 |

**说明**：执行 `lsscsi -g` 并按库配置的 drive 数筛选 VTL SCSI 节点。用于获取 changer 和 tape drive 对应的 `/dev/sg*` 设备路径，供 iSCSI 导出使用。

**响应：**
```json
{
  "library": "marstor",
  "transport": "local",
  "note": "scsi host5: 4 drives + 1 changer matched",
  "changer_sg": "sg3",
  "drive_sg": ["sg4", "sg5", "sg6", "sg7"],
  "drive_count": 4,
  "picked_scsi_host": 5,
  "devices": [
    { "role": "changer", "lun": 0, "sg": "sg3", "sch": "sch0" },
    { "role": "tape", "lun": 1, "sg": "sg4", "st": "st0", "index": 0 },
    { "role": "tape", "lun": 2, "sg": "sg5", "st": "st1", "index": 1 }
  ]
}
```

---

### 10.2 GET /api/manage/iscsi/scan-sg — iSCSI 兼容路径

**说明**：`/api/manage/iscsi/scan-sg` 与 `/api/manage/transport/scan-sg` 完全等价，为旧版兼容保留。

---

## 11. 系统监控

### 11.1 GET /api/monitor/system — 系统资源快照

**认证**：需要

**响应：**
```json
{
  "cpu": { "pct": 12.5, "num_cores": 8 },
  "mem": { "total_kb": 16777216, "used_kb": 4194304, "pct": 25.0 },
  "disks": [
    { "name": "sda", "read_bytes": 123456789, "write_bytes": 987654321 }
  ]
}
```

**数据来源**：`/proc/stat`、`/proc/meminfo`、`/proc/diskstats`。

---

### 11.2 GET /api/monitor/capacity-trend — 容量趋势

**认证**：需要

**查询参数：**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `library` | string | 全部 | 库名称（可选） |
| `limit` | int | 50 | 数据点数量上限 |

**响应：**
```json
{
  "points": [
    {
      "ts": "2026-06-01T10:00:00",
      "library": "marstor",
      "total_bytes": 1099511627776,
      "used_bytes": 549755813888,
      "tape_count": 10
    }
  ]
}
```

**说明**：从 `vtl_capacity_log` 表读取历史容量快照数据。

---

### 11.3 GET /api/monitor/events — 操作事件日志

**认证**：需要

**查询参数：**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `limit` | int | 50 | 返回条数上限 |
| `category` | string | 全部 | 按类别筛选（可选） |

**响应：**
```json
{
  "events": [
    {
      "id": 1234,
      "ts": "2026-06-01T10:30:00",
      "category": "tape",
      "action": "create",
      "detail": "Created tape 'TAPE001' (100G) in library marstor"
    }
  ]
}
```

---

## 12. 巡检

### 12.1 GET /api/patrol — 运行全栈巡检

**认证**：需要

**响应：**
```json
{
  "exit_code": 0,
  "stdout": "PASS: vtl.ko loaded\nPASS: /dev/vtl exists\n...",
  "stderr": "",
  "ok": ["vtl.ko loaded", "/dev/vtl exists", "DB integrity OK"],
  "warn": ["capacity usage > 80% on marstor"],
  "crit": []
}
```

**退出码含义**：`0`=全部正常，`1`=有警告，`2`=有严重问题

**巡检项目**：vtl.ko 加载状态、`/dev/vtl` 设备节点、DB 完整性、库几何与内核一致性、Web 服务可达、日志轮转状态。

---

## 13. 传输架构

### 13.1 GET /api/fabric — 获取传输架构信息

**认证**：需要

**响应：**
```json
{
  "transport": "local",
  "iscsi_iqn": null,
  "iscsi_portals": null,
  "fc_wwpn": null,
  "kernel_reload_on_db_change": false,
  "kernel_geom_prefer_ioctl": true,
  "vtl_reload_scan_delay_ms": 500,
  "log_max_bytes": 1048576,
  "iscsi_exports_in_db": [],
  "patrol_hint": "run: vtladm patrol  (or GET /api/patrol with session)",
  "product_limits": { ... }
}
```

---

### 13.2 GET /api/status — 获取库状态快照

**认证**：需要

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `library` | string | 是 | 库名称 |

**响应：**
```json
{
  "library": "marstor",
  "tape_count": 50,
  "loaded_in_drives": 2,
  "drives": 4,
  "data_slots": 32
}
```

---

## 14. 会话管理

### 14.1 GET /api/sessions — 获取活跃会话列表

**认证**：需要

**响应：**
```json
{
  "sessions": [
    {
      "token_prefix": "a1b2c3",
      "username": "admin",
      "created_secs_ago": 3600,
      "is_current": true
    }
  ],
  "count": 2
}
```

---

### 14.2 POST /api/sessions/revoke — 撤销会话

**认证**：需要

**请求体：**
```json
{
  "token": "a1b2c3d4..."
}
```

**说明**：撤销指定会话令牌。不可撤销自己当前会话。

---

## 15. 初始设置

### 15.1 GET /api/setup/status — 获取设置状态

**认证**：无需

**响应：**
```json
{
  "setup_required": true,
  "defaults": {
    "db_path": "/opt/vtladm/var/vtl.db",
    "tape_dir": "/opt/vtladm/var/tapes",
    "log_dir": "/opt/vtladm/var/log",
    "kernel_vtl_reload_script": "/opt/vtladm/sbin/vtl-kernelctl",
    "vtl_ko": "/opt/vtladm/ko/vtl.ko",
    "vtl_reload_scan_delay_ms": 500
  }
}
```

**说明**：如果 `setup_required=true`，前端应展示设置向导。已设置则前端直接跳转登录页。

---

### 15.2 POST /api/setup/complete — 完成设置

**认证**：无需

**请求体：**
```json
{
  "db_path": "/opt/vtladm/var/vtl.db",
  "tape_dir": "/opt/vtladm/var/tapes",
  "log_dir": "/opt/vtladm/var/log",
  "password": "admin_password_8chars_min",
  "vtl_ko": "/opt/vtladm/ko/vtl.ko",
  "kernel_vtl_reload_script": "/opt/vtladm/sbin/vtl-kernelctl",
  "personality": "vtl",
  "kernel_geometry_mode": "fixed"
}
```

**成功响应 (200)：**
```json
{
  "ok": true,
  "kernel_geom": "initialized"
}
```

---

## 16. 全局约定

### 16.1 CSRF 保护

所有非 GET 请求须在 Header 中携带 CSRF Token：
```
X-VTL-CSRF: <token_value>
```
CSRF Token 从 Cookie `vtl_csrf` 中读取。`/api/logout` 豁免此检查。

### 16.2 会话过期

- Session Cookie (`vtl_session`) 有效期 24 小时
- 401 响应 = 前端应跳转 `/login`
- 关闭浏览器页面时，`beforeunload` 事件触发 `sendBeacon` 到 `/api/logout`，实现关闭即登出

### 16.3 槽位编号约定

- 内部使用 **0-based** 编号
- data_slots 范围：`0` 到 `num_data_slots - 1`
- 驱动器地址：`1000 + drive_index`
- I/E 槽地址：`2000 + mail_index`

### 16.4 容量格式

支持带单位的容量字符串，大小写不敏感：
- `B` — 字节
- `K` / `KB` — 千字节 (1024)
- `M` / `MB` — 兆字节
- `G` / `GB` — 吉字节
- `T` / `TB` — 太字节

### 16.5 密度格式

| 标签 | 十六进制 | T10 代码 | 容量范围 |
|---|---|---|---|
| Default LTO | 0x40 | — | 100M–1T |
| LTO-5 | 0x4A | — | 1G–3T |
| LTO-6 | 0x4C | — | 1G–6T |
| LTO-7 | 0x4E | — | 1G–15T |
| LTO-8 | 0x50 | — | 1G–30T |
| LTO-9 | 0x52 | — | 1G–45T |
| LTO-10 | 0x58 | — | 1G–90T |

### 16.6 产品上限

| 限制项 | 值 |
|---|---|
| 最大在线库数 | 8 |
| 每库最大驱动器数 | 8 |
| 每库最大数据槽位数 | 256 |

### 16.7 库名 `__offline__` 约定

`__offline__` 是系统保留的离线保管库，用于管理已从机械手卸下的磁带。它有货架但无机械手槽位，不出现在 SCSI 导出中。

---

## 附录 A：LibraryDetailResponse

```json
{
  "library": {
    "id": 1,
    "name": "marstor",
    "created_at": "2025-01-15T10:30:00",
    "is_offline_storage": false,
    "tape_count": 50,
    "loaded_in_drives": 2,
    "drive_count": 4,
    "data_slots": 32,
    "mail_slots": 4,
    "max_drives": 8,
    "slots": 32,
    "can_delete_online": true,
    "inventory_source": "kernel",
    "inventory_truncated": false
  },
  "drives": [
    { "drive_id": 0, "tape_name": "TAPE001", "tape_barcode": "VTL00A1B2" },
    { "drive_id": 1, "tape_name": null, "tape_barcode": null }
  ],
  "tapes": [
    {
      "name": "TAPE001",
      "barcode": "VTL00A1B2",
      "capacity_bytes": 107374182400,
      "used_bytes": 0,
      "slot": 5,
      "shelf_name": "unused",
      "in_drive": true
    }
  ],
  "changer": {
    "source": "kernel",
    "data_slots": [
      { "label": "Slot 0", "tape_name": "TAPE002", "barcode": "VTL00C3D4" },
      { "label": "Slot 1", "tape_name": null, "barcode": null }
    ],
    "drives": [
      { "label": "Drive 0", "tape_name": "TAPE001", "barcode": "VTL00A1B2" }
    ],
    "mailslots": [
      { "label": "IE 0", "tape_name": null, "barcode": null }
    ]
  }
}
```

---

## 附录 B：API 端点速查表

| 方法 | 路径 | 认证 | 说明 |
|---|---|---|---|
| GET | `/api/captcha` | 否 | 获取算术验证码 |
| POST | `/api/login` | 否 | 登录 |
| POST | `/api/logout` | 是 | 登出（豁免 CSRF） |
| POST | `/api/change-password` | 是 | 修改密码 |
| GET | `/api/session/ping` | 否 | 检查会话有效性 |
| GET | `/api/setup/status` | 否 | 获取设置状态 |
| POST | `/api/setup/complete` | 否 | 完成初始设置 |
| GET | `/api/sessions` | 是 | 获取活跃会话 |
| POST | `/api/sessions/revoke` | 是 | 撤销会话 |
| GET | `/api/libraries` | 是 | 获取磁带库列表 |
| GET | `/api/libraries-status` | 是 | 获取库状态摘要 |
| GET | `/api/library/detail` | 是 | 获取库详情 |
| POST | `/api/manage/library/create` | 是 | 创建磁带库 |
| POST | `/api/manage/library/delete` | 是 | 删除磁带库 |
| GET | `/api/tapes` | 是 | 获取磁带列表（分页） |
| GET | `/api/manage/tape/density-limits` | 是 | 获取密度容量范围 |
| POST | `/api/manage/tape/create` | 是 | 创建磁带 |
| POST | `/api/manage/tape/create-batch` | 是 | 批量创建磁带 |
| POST | `/api/manage/tape/create-auto-batch` | 是 | 自动批量建带 |
| POST | `/api/manage/tape/delete` | 是 | 删除磁带 |
| POST | `/api/manage/tape/init` | 是 | 清空磁带 |
| POST | `/api/manage/tape/shelf-place` | 是 | 单盘换架 |
| POST | `/api/manage/tape/shelf-place-batch` | 是 | 批量换架 |
| POST | `/api/manage/tape/migrate-shelves-batch` | 是 | 批量迁移货架 |
| GET | `/api/shelves` | 是 | 获取货架列表 |
| GET | `/api/offline-shelves` | 是 | 获取离线货架 |
| POST | `/api/manage/shelf/create` | 是 | 创建货架 |
| POST | `/api/manage/shelf/delete` | 是 | 删除货架 |
| POST | `/api/manage/shelf/create-offline` | 是 | 创建离线货架 |
| GET | `/api/empty-slots` | 是 | 获取空槽位 |
| POST | `/api/manage/tape/assign-slot` | 是 | 单盘入槽 |
| POST | `/api/manage/tape/assign-slot-batch` | 是 | 批量入槽 |
| POST | `/api/manage/tape/load` | 是 | 装载磁带到驱动器 |
| POST | `/api/manage/tape/unload` | 是 | 从驱动器卸载 |
| POST | `/api/manage/tape/eject` | 是 | 弹出到 I/E 槽 |
| POST | `/api/manage/robot/reconcile` | 是 | 对账 |
| POST | `/api/manage/robot/auto-align` | 是 | 自动对齐 |
| POST | `/api/manage/robot/sync` | 是 | 同步 DB→内核 |
| GET | `/api/fabric` | 是 | 获取传输架构 |
| GET | `/api/status` | 是 | 获取库状态 |
| GET | `/api/patrol` | 是 | 运行巡检 |
| GET | `/api/manage/iscsi/config` | 是 | iSCSI 配置 |
| POST | `/api/manage/iscsi/allow-exec` | 是 | 设置执行权限 |
| POST | `/api/manage/iscsi/check` | 是 | 检查环境 |
| GET | `/api/manage/iscsi/library-export-defaults` | 是 | 导出默认值 |
| POST | `/api/manage/iscsi/library-export` | 是 | 导出库 |
| POST | `/api/manage/iscsi/library-unexport` | 是 | 卸载导出 |
| POST | `/api/manage/iscsi/quick-export` | 是 | 快速导出 |
| POST | `/api/manage/iscsi/quick-unexport` | 是 | 快速卸载 |
| GET | `/api/manage/transport/scan-sg` | 是 | SCSI 扫描 |
| GET | `/api/manage/iscsi/scan-sg` | 是 | SCSI 扫描（兼容） |
| GET | `/api/monitor/system` | 是 | 系统资源快照 |
| GET | `/api/monitor/capacity-trend` | 是 | 容量趋势 |
| GET | `/api/monitor/events` | 是 | 操作事件日志 |
