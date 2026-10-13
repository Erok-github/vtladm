# VTL Web UI Vue3 重构 — 需求文档

## 1. 当前系统概述

### 1.1 现有架构

| 层面 | 技术 | 问题 |
|------|------|------|
| 后端框架 | Axum (Rust) | — |
| 前端渲染 | 服务端 HTML 拼接 (`concat!` + `include_str!`) | 维护性极差，Rust 内嵌 HTML 字符串 |
| CSS | `web_shell.css` (606行) — 单文件，无预处理 | 无组件隔离，样式全局污染 |
| JS | `web_boot.js` (48行) — 仅 CSRF + toast | 无前端状态、无交互框架 |
| 交互模型 | `<form>` 提交 → 整页重载 | 用户体验差，无加载态 |
| 鉴权 | Session cookie + CSRF (web_auth.rs) | 需保留 |

### 1.2 数据模型

```
Library (磁带库)
├── id, name, geometry (drives × slots)
├── Drives (驱动器, 1..N)
│   └── loaded_tape_id, source_slot
├── Slots (机械手槽位, 1..M)
│   └── tape_id (nullable)
└── Mailslots (导入导出槽, 固定 4)

Tape (磁带)
├── name, barcode, serial
├── capacity_bytes, used_bytes
├── density (T10 码: LTO-4..LTO-10)
├── meta_flags (压缩: LZO/zlib)
├── shelf_id → Shelf (货架归属)
├── slot (内核槽位号, nullable)
└── 磁盘文件: <name>.vtltape + <name>.vtlmeta

Shelf (货架)
├── id, name (唯一)
├── is_offline (在线/离线存储)
└── tapes: Vec<Tape>

Kernel Changer (内核 /dev/vtl)
├── 与单核 DB 通过 GET_INVENTORY ioctl 同步
├── 状态文件: /opt/vtladm/var/changer-<id>.state
└── 对账机制: kernel→DB (pull), 不支持 DB→kernel (push)
```

### 1.3 现有页面列表 (18 页)

**浏览区 (Viewer)**
| 路径 | 页面 | 说明 |
|------|------|------|
| `/` | 首页 | 空壳，仅侧栏 |
| `/browse/tapes` | 磁带目录 | 库内磁带列表 |
| `/browse/status` | 库状态 | 槽位/驱动器占用状态 |
| `/browse/fabric` | 传输配置 | SCSI / iSCSI 配置 JSON |

**管理区 (Admin)**
| 路径 | 页面 | 说明 |
|------|------|------|
| `/admin/overview` | 后台概览 | 9 个卡片入口 |
| `/admin/library` | 建库管理 | CRUD 库 |
| `/admin/tapes` | 磁带与货架 | 批量建带/删带/改架 (5 个 Tab) |
| `/admin/shelf` | 货架管理 | CRUD 货架 |
| `/admin/assign-slot` | 磁带入槽 | 货架→在线槽位 |
| `/admin/changer` | 对账 | DB↔内核 inventory 比较/修复 |
| `/admin/shelf-place` | 磁带出库 | 在线库→离线货架 |
| `/admin/transport` | 传输向导 | SCSI 发现、链路指引 |
| `/admin/iscsi` | iSCSI 映射 | iSCSI 导出/撤销 |
| `/admin/account` | 账户安全 | 修改密码 |
| `/admin/setup-init` | 初始设置 | 首次登录设置 |
| `/login` | 登录 | 算术验证码 |

### 1.4 现有 API 端点 (80+)

全部 REST JSON 端点，按功能分组：
- **查询**: `GET /api/libraries`, `/api/tapes`, `/api/status`, `/api/fabric`, `/api/shelves`, `/api/empty-slots`, `/api/patrol`, `/api/library/detail`
- **管理**: `POST /api/manage/library/create|delete`, `/api/manage/tape/create|delete|init`, `/api/manage/shelf/create|delete`
- **操作**: `POST /api/manage/tape/assign-slot|load|unload|eject|shelf-place`
- **批量**: `POST /api/manage/tape/{create-batch|create-auto-batch|assign-slot-batch|shelf-place-batch|migrate-shelves-batch}`
- **对账**: `POST /api/manage/robot/{sync|reconcile|auto-align}`
- **传输**: `POST /api/manage/iscsi/{quick-export|quick-unexport|library-export|library-unexport}`
- **鉴权**: `POST /api/login|logout|change-password`, `GET /api/captcha|sessions|setup/status`

---

## 2. Vue3 重构目标

### 2.1 非目标（保持不变）

- Axum 后端 Rust 代码 **不改动**
- 全部现有 API 端点 **保持不变**
- Session/cookie 鉴权机制 **不变**
- SQLite 数据模型与内核 ioctl 接口 **不变**
- `vtladm serve` 启动方式 **不变**

### 2.2 目标

1. **前后端分离**: Rust 仅提供 API + 静态文件服务，前端由 Vue3 SPA 接管
2. **组件化**: 复用表格、表单、对话框、状态卡片等 UI 单元
3. **响应式交互**: 操作即反馈（loading → success/error toast），无整页刷新
4. **状态管理**: Pinia 管理全局状态（当前库、用户会话、磁带列表缓存）
5. **现代工程化**: TypeScript + Vite + ESLint，源码可维护
6. **保留所有功能**: 对账、批量建带、iSCSI 导出等专业功能完整迁移
7. **向后兼容**: 构建产物为纯静态文件，由 Axum 的 `tower_http::services::ServeDir` 托管

---

## 3. 功能需求细则

### 3.1 全局导航

- 左侧可折叠树形导航，按工作流分组：
  - 浏览：仪表盘、磁带浏览、库状态、传输视图
  - 管理：建库、磁带管理、槽位分配、对账、货架
  - 传输：iSCSI 管理、SCSI 扫描
  - 系统：账户安全
- 顶部库选择器（下拉切换当前操作库）
- 面包屑导航
- 用户会话指示器（已登录 + 登出按钮）

### 3.2 仪表盘 (Dashboard) — 替代首页 + 后台概览

- 统计卡片：库数量、磁带总数、已用容量、在线驱动器
- 最近操作日志（来自 patrol 摘要）
- 各库概览卡片（槽位占用率、驱动器状态）
- 快速操作入口
- **自动刷新**: 可配置间隔（默认关闭），轮询 `/api/libraries-status`

### 3.3 库管理

- 表格列出所有在线库（名称、驱动器数、槽位数、磁带数、创建时间）
- 新建库对话框（名称 + 驱动器数 + 槽位数，含密度联动校验）
- 删除库（确认对话框 + 非空警告）
- 库详情面板（展开行）：各驱动器/槽位占用状态、磁带列表

### 3.4 磁带管理 (核心)

- **磁带列表**：分页表格（名称、条码、容量、已用、密度、压缩、所在架/槽/驱动器）
- **筛选**：按货架、密度、状态（在线/离线/在驱动器）
- **批量操作**：多选 → 批量建带/删带/换架/入槽
- **建带对话框**：
  - 名称、容量（带单位解析：100G/1T）
  - 密度下拉（LTO-4 ~ LTO-10，带容量提示）
  - 压缩开关 + 算法选择（zlib/lzo）
  - 目标货架
- **磁带操作**：入槽、装入驱动器、卸载、弹出、清空（init）、导出
- **详情面板**：展开行显示序列号、创建时间、最近访问、读写计数器

### 3.5 机械手/对账 (Changer)

- 内核 inventory 快照 vs DB 对比表格
- 漂移项高亮（DB 与内核不一致的行）
- 操作按钮：Pull (kernel→DB)、Auto-Align、Sync-DB
- 操作结果摘要（修复数、剩余漂移数）
- **实时刷新**：操作后自动重拉 inventory

### 3.6 货架管理

- 在线/离线货架分 Tab 展示
- 每架磁带列表 + 占用率
- 新建/删除货架
- 磁带换架（拖拽或下拉选择）

### 3.7 传输管理 (iSCSI/SCSI)

- 当前 VTL SCSI 设备列表（`lsscsi -g` 输出）
- iSCSI 快速导出/撤销
- 库级 iSCSI 导出配置（IQN 生成、target 配置）
- SCSI 重新扫描触发

### 3.8 账户安全

- 修改密码表单
- 活跃会话列表
- 撤销会话

### 3.9 登录页

- 算术验证码（保留现有机制）
- 用户名/密码
- 错误提示

---

## 4. 非功能需求

### 4.1 性能

- SPA 初始加载 ≤ 2s（gzip 后 bundle ≤ 500KB）
- 表格支持虚拟滚动（磁带数 > 1000 时）
- API 响应缓存（库列表、货架列表等低频变更数据）
- Toast 通知非阻塞

### 4.2 兼容性

- 支持 Chrome/Firefox/Edge 最近 2 个大版本
- 不要求移动端适配（管理工具，桌面优先）
- 最低分辨率 1280×720

### 4.3 安全

- CSRF token 在所有 POST 请求中携带
- Session cookie HttpOnly + Secure（可配置）
- 登出清除 session
- XSS 防护：Vue 默认转义 + CSP header

### 4.4 可维护性

- TypeScript strict 模式
- 组件粒度：页面 → 业务组件 → 基础 UI 组件
- API 调用集中在 `api/` 目录，类型定义在 `types/`
- Pinia store 按领域拆分（library, tape, shelf, auth, changer）

---

## 5. 技术栈选型

| 类别 | 选择 | 理由 |
|------|------|------|
| 框架 | Vue 3.5+ (Composition API) | 用户指定 |
| 语言 | TypeScript 5.x strict | 类型安全 |
| 构建 | Vite 6 | 快、生态好 |
| UI 组件库 | Naive UI | 企业级、中文友好、暗色模式 |
| 状态管理 | Pinia | Vue 3 官方推荐 |
| 路由 | Vue Router 4 | 标准方案 |
| HTTP | 封装 fetch（带 CSRF + 错误处理） | 零依赖 |
| 表格 | Naive UI DataTable + 自定义列 | 内置排序/筛选/分页 |
| 图表 | 轻量 CSS 进度条（槽位占用率等） | 无需重型图表库 |
| 图标 | Naive UI 内置图标 + @vicons/ionicons5 | 统一风格 |
| 测试 | Vitest + Vue Test Utils | 可选 |

### 5.1 构建部署

```
vtladm userspace/
├── src/
│   ├── main.rs (Axum serve 增加静态文件托管)
│   └── ... (现有代码不动)
├── web/                          ← 新增 Vue3 前端
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.ts
│       ├── App.vue
│       ├── router/
│       ├── stores/
│       ├── api/
│       ├── components/
│       ├── views/
│       └── types/
└── Cargo.toml
```

构建产物输出到 `userspace/web/dist/`，Rust 通过 `ServeDir` 托管：
```rust
// main.rs serve 命令中增加：
Router::new()
    .nest_service("/", ServeDir::new("web/dist"))
    .nest("/api", api_router)
    .fallback(|| async { /* SPA fallback to index.html */ })
```

---

## 6. 迁移策略

### 分阶段实施

| 阶段 | 内容 | 风险 |
|------|------|------|
| **Phase 1** | 项目脚手架 + 登录 + 仪表盘 + 库列表 | 低 |
| **Phase 2** | 磁带管理 CRUD + 批量操作 + 货架 | 中 |
| **Phase 3** | 对账面板 + 传输/iSCSI + 账户 | 中 |
| **Phase 4** | 抛光（暗色模式、虚拟滚动、自动刷新、错误恢复） | 低 |

### 切换方案

- 新旧 UI 并行运行（不同端口或路径前缀）
- `/api/*` 端点共享
- 验证通过后移除旧 HTML 常量代码
