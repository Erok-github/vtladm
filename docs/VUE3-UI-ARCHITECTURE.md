# VTL Web UI Vue3 重构 — 技术方案与架构设计

## 1. 总体架构

### 拓扑图

```
┌──────────────────────────────────────────────────────────────┐
│                        用户浏览器                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │               Vue3 SPA (Vite 构建)                       │ │
│  │  ┌─────────┐ ┌──────────┐ ┌────────────┐ ┌───────────┐ │ │
│  │  │  Router  │ │  Pinia   │ │Components │ │ Naive UI │ │ │
│  │  │ VueRouter│ │  Stores  │ │  (60+)    │ │(组件库)   │ │ │
│  │  └─────────┘ └──────────┘ └────────────┘ └───────────┘ │ │
│  └─────────────────────────────────────────────────────────┘ │
│                           │ HTTP (fetch)                      │
└───────────────────────────┼──────────────────────────────────┘
                            │
┌───────────────────────────┼──────────────────────────────────┐
│                     Axum (Rust)                               │
│  ┌──────────────────────┐  ┌───────────────────────────────┐ │
│  │  /api/* (不变)        │  │  / ← ServeDir("web/dist")     │ │
│  │  80+ REST 端点       │  │  SPA fallback → index.html   │ │
│  └──────────────────────┘  └───────────────────────────────┘ │
│         │                                                     │
│    ┌────┴─────┐                                              │
│    │ SQLite   │──── DB 操作                                   │
│    └──────────┘                                              │
│    ┌──────────┐                                              │
│    │ /dev/vtl │──── 内核 ioctl                                │
│    └──────────┘                                              │
└──────────────────────────────────────────────────────────────┘
```

### 关键设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 部署方式 | 前端构建产物由 Axum 托管 | 零运维依赖，单进程部署 |
| 数据获取 | 完全复用现有 `/api/*` | 后端零改动，只需增加 `ServeDir` |
| 路由模式 | HTML5 History Mode | 干净 URL；Axum 需 fallback |
| 组件库 | Naive UI | 企业风格、Tree/Table 组件成熟、暗色模式内置 |
| 状态管理 | Pinia 按领域拆分 | 比 Vuex 更简洁，完整 TS 支持 |

---

## 2. Axum 后端改动

### 2.1 新增依赖

```toml
# Cargo.toml
tower-http = { version = "0.5", features = ["fs", "cors"] }
```

### 2.2 serve 命令改动

```rust
// main.rs serve 子命令，在现有 Axum Router 基础上：
use tower_http::services::ServeDir;
use tower_http::cors::CorsLayer;

async fn run_web_ui(host: &str, port: u16, auth: Arc<WebState>) {
    let spa_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/dist");

    // API 路由（现有，不变）
    let api_router = build_web_router(auth.clone());

    let app = Router::new()
        // API 优先匹配
        .nest("/api", api_router)
        // 静态文件 (Vue 构建产物)
        .nest_service("/", ServeDir::new(&spa_dir))
        // SPA fallback：非 /api 路径都返回 index.html
        .fallback_service(ServeFile::new(spa_dir.join("index.html")));

    let listener = tokio::net::TcpListener::bind((host, port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### 2.3 原有代码处理

- `web.rs` 中的 HTML 页面常量（`HOME_HTML`, `ADMIN_TAPES_HTML` 等 18 个）标记 `#[allow(dead_code)]` 保留，等验证通过后删除
- `web_shell.css`、`web_boot.js`、`web_*_inner.html` 保留到清理阶段

---

## 3. Vue3 前端项目结构

```
web/
├── index.html                      # Vite 入口 HTML
├── package.json
├── vite.config.ts
├── tsconfig.json
├── env.d.ts
├── public/
│   └── favicon.ico
└── src/
    ├── main.ts                     # createApp + router + pinia + naive
    ├── App.vue                     # 根组件（Layout 壳）
    │
    ├── router/
    │   └── index.ts                # 路由配置
    │
    ├── stores/                     # Pinia
    │   ├── auth.ts                 # 登录态、session 信息
    │   ├── library.ts              # 当前库、库列表
    │   ├── tapes.ts                # 磁带数据缓存
    │   ├── changer.ts              # 内核 inventory
    │   └── ui.ts                   # 加载状态、全局 toast
    │
    ├── api/                        # HTTP 封装
    │   ├── client.ts               # fetch 封装（CSRF、错误处理、baseUrl）
    │   ├── auth.ts                 # login/logout/captcha/changePassword
    │   ├── libraries.ts            # 库 CRUD
    │   ├── tapes.ts                # 磁带 CRUD + 批量
    │   ├── changer.ts              # 对账/auto-align/sync
    │   ├── shelves.ts              # 货架管理
    │   ├── transport.ts            # iSCSI/SCSI 传输
    │   └── types.ts                # 所有 API 请求/响应 TS 类型
    │
    ├── components/                 # 通用组件
    │   ├── layout/
    │   │   ├── AppLayout.vue       # 整体布局（侧栏 + 主内容）
    │   │   ├── SideNav.vue         # 左侧树形导航
    │   │   ├── TopBar.vue          # 顶部栏（库选择器 + 用户）
    │   │   └── Breadcrumb.vue      # 面包屑
    │   ├── common/
    │   │   ├── DataTable.vue       # 通用表格封装
    │   │   ├── ConfirmDialog.vue   # 确认对话框
    │   │   ├── StatusBadge.vue     # 状态徽章
    │   │   ├── CapacityBar.vue     # 容量进度条
    │   │   ├── DensitySelect.vue   # 密度选择器
    │   │   ├── ShelfSelect.vue     # 货架选择器
    │   │   ├── LibrarySelect.vue   # 库选择器
    │   │   └── EmptyState.vue      # 空状态占位
    │   └── forms/
    │       ├── TapeCreateForm.vue  # 建带表单
    │       ├── LibraryCreateForm.vue # 建库表单
    │       └── ShelfCreateForm.vue # 建架表单
    │
    └── views/                      # 页面
        ├── login/
        │   └── LoginView.vue
        ├── dashboard/
        │   └── DashboardView.vue
        ├── libraries/
        │   ├── LibraryListView.vue
        │   └── LibraryDetailView.vue
        ├── tapes/
        │   ├── TapeListView.vue
        │   └── TapeDetailPanel.vue
        ├── changer/
        │   └── ChangerReconcileView.vue
        ├── shelves/
        │   └── ShelfManageView.vue
        ├── assign/
        │   └── AssignSlotView.vue
        ├── transport/
        │   └── TransportView.vue
        ├── account/
        │   └── AccountView.vue
        └── setup/
            └── SetupWizardView.vue
```

---

## 4. 路由设计

```typescript
// router/index.ts
const routes = [
  {
    path: '/login',
    component: () => import('@/views/login/LoginView.vue'),
    meta: { public: true }
  },
  {
    path: '/',
    component: () => import('@/components/layout/AppLayout.vue'),
    children: [
      { path: '', name: 'dashboard', component: () => import('@/views/dashboard/DashboardView.vue') },
      { path: 'libraries', name: 'libraries', component: () => import('@/views/libraries/LibraryListView.vue') },
      { path: 'libraries/:id', name: 'library-detail', component: () => import('@/views/libraries/LibraryDetailView.vue') },
      { path: 'tapes', name: 'tapes', component: () => import('@/views/tapes/TapeListView.vue') },
      { path: 'changer', name: 'changer', component: () => import('@/views/changer/ChangerReconcileView.vue') },
      { path: 'shelves', name: 'shelves', component: () => import('@/views/shelves/ShelfManageView.vue') },
      { path: 'assign-slot', name: 'assign-slot', component: () => import('@/views/assign/AssignSlotView.vue') },
      { path: 'transport', name: 'transport', component: () => import('@/views/transport/TransportView.vue') },
      { path: 'account', name: 'account', component: () => import('@/views/account/AccountView.vue') },
      { path: 'setup', name: 'setup', component: () => import('@/views/setup/SetupWizardView.vue') },
    ]
  }
];
```

路由守卫：
- 非 `public` 路由 → 检查 `auth` store 是否有有效 session
- 无 session → 跳转 `/login`
- setup 未完成 → 强制跳转 `/setup`
- 必须修改密码 → 强制跳转 `/account`

---

## 5. Pinia Store 设计

### 5.1 authStore

```typescript
interface AuthState {
  sessionValid: boolean;
  username: string;
  mustChangePassword: boolean;
  csrfToken: string;
  captchaId: string | null;
  captchaQuestion: string | null;
}

// Actions: login(), logout(), fetchCaptcha(), changePassword()
```

### 5.2 libraryStore

```typescript
interface LibraryState {
  currentLibrary: string | null;   // 当前选中库名
  libraries: Library[];            // 所有在线库
  libraryDetail: LibraryDetail | null;
  loading: boolean;
}

// Actions: fetchLibraries(), fetchDetail(), switchLibrary()
// Getters: currentLibraryId, isOffline
```

### 5.3 tapeStore

```typescript
interface TapeState {
  tapes: Tape[];
  total: number;
  offset: number;
  limit: number;
  loading: boolean;
  selectedIds: Set<string>;
}

// Actions: fetchTapes(), createTape(), deleteTape(), batchCreate(),
//          assignSlot(), load(), unload(), eject(), shelfPlace()
```

### 5.4 changerStore

```typescript
interface ChangerState {
  dbInventory: Map<string, Location>;
  kernelInventory: Map<string, Location>;
  drifts: DriftItem[];
  loading: boolean;
}

// Actions: fetchInventory(), reconcile(), autoAlign(), syncDb()
```

---

## 6. 组件树示例（TapeListView）

```
TapeListView.vue
├── TopBar.vue
│   ├── LibrarySelect.vue          # 库切换
│   └── NButton (登出)
├── NTabs
│   ├── Tab: "磁带列表"
│   │   ├── Toolbar
│   │   │   ├── NButton (建带)
│   │   │   ├── NButton (批量入槽) [disabled when no selection]
│   │   │   ├── NButton (批量删带) [disabled when no selection]
│   │   │   └── NInput (搜索)
│   │   ├── NDataTable
│   │   │   ├── Column: check (NCheckbox)
│   │   │   ├── Column: 名称 (NSpace: 名称 + 条码)
│   │   │   ├── Column: 容量 (CapacityBar)
│   │   │   ├── Column: 密度 (NTag)
│   │   │   ├── Column: 压缩 (StatusBadge)
│   │   │   ├── Column: 位置 (货架/槽位/驱动器)
│   │   │   └── Column: 操作 (NDropdown: 入槽/装入/卸载/弹出/删除)
│   │   └── NPagination
│   └── Tab: "批量建带"
│       └── TapeCreateForm.vue
│           ├── NForm
│           │   ├── NInputNumber (数量)
│           │   ├── NInput (名称前缀)
│           │   ├── NInput (容量)
│           │   ├── DensitySelect
│           │   ├── NSwitch (压缩)
│           │   ├── NSelect (算法: zlib/lzo)
│           │   └── ShelfSelect
│           └── NButton (提交)
```

---

## 7. API 客户端设计 (api/client.ts)

```typescript
const BASE = '';  // 同源，无跨域

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {};

  if (method !== 'GET') {
    // 从 cookie 读取 CSRF token
    const csrf = document.cookie.match(/vtl_csrf=([^;]*)/)?.[1] ?? '';
    headers['X-VTL-CSRF'] = csrf;
    headers['Content-Type'] = 'application/json';
  }

  const resp = await fetch(`${BASE}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
    credentials: 'include',
  });

  if (resp.status === 401) {
    // session 过期 → 跳转登录
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  const data = await resp.json();
  if (!resp.ok) {
    throw new ApiError(resp.status, data.error || data.message || 'Unknown error');
  }

  return data as T;
}

export const api = {
  get<T>(path: string) { return request<T>('GET', path); },
  post<T>(path: string, body?: unknown) { return request<T>('POST', path, body); },
};
```

---

## 8. 关键交互模式

### 8.1 操作确认→执行→反馈

```
用户点击 "删除磁带"
  → 弹出 ConfirmDialog: "确定删除磁带 foo？此操作不可撤销。"
    → 用户点 "确定"
      → 按钮变 loading 态
      → api.post('/api/manage/tape/delete', { library, name })
        → 成功: toast.success("磁带 foo 已删除") + 刷新列表
        → 失败: toast.error("删除失败: 磁带在驱动器中")
```

### 8.2 对账视图（ChangerReconcileView）

```
进入页面
  → fetchInventory() 并行拉取 DB + Kernel inventory
  → 构建对比视图：
    ┌──────────────┬──────────┬──────────┬──────────┐
    │ 磁带         │ DB 位置   │ 内核位置  │ 状态     │
    ├──────────────┼──────────┼──────────┼──────────┤
    │ tape01       │ Slot 3   │ Slot 3   │ ✓ 一致   │
    │ tape02       │ Slot 5   │ Drive 0  │ ✗ 漂移   │
    │ tape03       │ Shelf A  │ Slot 7   │ ✗ 漂移   │
    └──────────────┴──────────┴──────────┴──────────┘

  → 漂移行高亮橙色
  → 操作按钮:
    [Pull from Kernel]  [Auto-Align]  [Sync DB]
  → 操作后自动刷新 inventory，重新计算漂移
```

### 8.3 加载态与空状态

```
加载中:       <NSpin /> 全屏遮罩 或 <NSkeleton /> 占位
空数据:       <EmptyState description="暂无磁带，请先建带" />
错误:         <NResult status="500" title="加载失败">
              <NButton @click="retry">重试</NButton>
```

---

## 9. 构建配置

### vite.config.ts

```typescript
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  base: '/',  // 根路径
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    rollupOptions: {
      output: {
        manualChunks: {
          'naive-ui': ['naive-ui'],
          'vendor': ['vue', 'vue-router', 'pinia'],
        },
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8765',  // 开发时代理到 Axum
    },
  },
});
```

### package.json

```json
{
  "name": "vtladm-web",
  "version": "1.0.0",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "vue": "^3.5",
    "vue-router": "^4.4",
    "pinia": "^2.2",
    "naive-ui": "^2.40"
  },
  "devDependencies": {
    "@vitejs/plugin-vue": "^5.1",
    "typescript": "^5.6",
    "vite": "^6.0",
    "vue-tsc": "^2.1"
  }
}
```

---

## 10. 迁移分阶段计划

### Phase 1 — 基础框架（2-3 天）

- [ ] Vite + Vue3 + TypeScript 脚手架搭建
- [ ] Naive UI 集成，全局主题配置
- [ ] router + pinia 基础结构
- [ ] api/client.ts fetch 封装（CSRF、鉴权）
- [ ] AppLayout（侧栏导航 + 顶栏 + 内容区）
- [ ] LoginView（captcha 登录）
- [ ] Axum 增加 ServeDir + SPA fallback
- [ ] **验证**: 登录流程完整可用

### Phase 2 — 数据浏览（2-3 天）

- [ ] DashboardView（统计卡片 + 快速入口）
- [ ] LibraryListView + LibraryDetailView
- [ ] TapeListView（分页表格 + 筛选 + 容量进度条）
- [ ] TapeDetailPanel（展开行，序列号、时间戳、计数器）
- [ ] ShelfManageView
- [ ] **验证**: 所有现有 API 返回数据正确渲染

### Phase 3 — 操作与对账（3-4 天）

- [ ] TapeCreateForm（含密度联动、压缩开关）
- [ ] 批量建带、批量入槽
- [ ] load/unload/eject 操作
- [ ] ChangerReconcileView（DB↔内核对账面板）
- [ ] AssignSlotView
- [ ] **验证**: 对账/操作功能与旧 UI 行为一致

### Phase 4 — 传输与账户（2 天）

- [ ] TransportView（SCSI 扫描、iSCSI 导出）
- [ ] AccountView（密码修改、会话管理）
- [ ] SetupWizardView（首次设置向导）
- [ ] **验证**: iSCSI 导出流程正常

### Phase 5 — 抛光（1-2 天）

- [ ] 暗色模式切换（Naive UI 内置）
- [ ] 大磁带列表虚拟滚动（> 1000 条时）
- [ ] 操作日志/patrol 结果展示
- [ ] 错误恢复（网络断开重连提示）
- [ ] 清理旧 HTML 模板代码（web.rs 中的 18 个 HTML 常量）
- [ ] **验证**: 完整回归测试

---

## 11. 风险与缓解

| 风险 | 概率 | 缓解 |
|------|------|------|
| 现有 API 返回格式不统一 | 中 | Phase 1 先调查所有 API 响应类型，统一 `api/types.ts` |
| CSRF cookie 读取失败在 SPA | 低 | 现有 `web_boot.js` 已验证可行；fetch 同源携带 cookie 无问题 |
| Naive UI Tree 组件不满足导航需求 | 低 | 侧栏用自定义组件 + NMenu，不依赖 Tree |
| 暗色模式与现有 CSS 冲突 | 低 | 完全移除 `web_shell.css`，纯 Naive UI 主题 |
| 构建产物过大 | 低 | manualChunks 拆包，gzip 后通常 < 300KB |
