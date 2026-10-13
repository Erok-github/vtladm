# vtladm 虚拟磁带库（VTL）项目代码审查报告（第三轮 · 完整审查收尾）

- 审查日期：2026-10-13（第三轮，接续 `code-review-report.md`、`code-review-report-2.md`）
- 审查范围：前两轮**未逐行覆盖**的剩余全部源码——`userspace/web`（Vue3 前端，约 2300 行）、`web.rs` 遗留 server-rendered 管理页与全部管理 API、`main.rs` 剩余函数面（setup 向导、库/带名校验、路径处理、`link_kernel_tapes`、删库事务、`CURRENT_LIBRARY` 并发模型）、`robot_sync.rs` / `reconcile.rs` / `monitor.rs` / `label.rs` / `scsi_rescan_vtl.rs`（约 2.5k 行）、`Cargo.toml` 依赖治理、packaging 脚本复核。
- 审查方式：纯静态审查。本机为 Windows，内核模块与 Rust 用户态以 Linux 为目标，未做编译与运行验证。
- 代码基线：分支 `vtladm26`，提交 `03ab310`（含前两轮全部 H/M/L 修复）。

---

## 一、本轮新发现

### L14（低危，已修复）~~遗留管理页将服务器错误消息未转义插入 innerHTML~~

> **状态更新**：已在 `59871db` 修复——`web.rs` 遗留页 3 处错误分支（约 4429/4439/4494 行）的 `j.error`/`tj.error` 渲染统一加 `escapeHtml()`（`showToast` 路径本就走 `textContent`，无需改）。

`userspace/src/web.rs` 遗留 server-rendered 管理页（`BROWSE_TAPES_HTML`、`ADMIN_TAPES_HTML` 等）约 6 处错误分支曾把 `j.error` 直接拼入 `innerHTML`（如 `innerHTML='<p class="err">'+(j.error||r.status)+'</p>'`），未走同文件已定义的 `escapeHtml()`。

服务器错误消息会回显用户输入：典型链路是货架名——`create_shelf`（`main.rs:6059`）对货架名**原本无任何字符白名单**（仅空名/保留名检查），创建重名货架时错误消息为 `Shelf '<name>' may already exist`，其中 `<name>` 可含任意 HTML。该错误经 API 回到管理页后以 `innerHTML` 渲染，构成**已认证场景下的反射型 XSS 通道**（管理员自伤或配合 CSRF 会话固定）。修复后该链路关闭。

### L15（低危，已修复）~~货架名缺失字符白名单~~

> **状态更新**：已在 `59871db` 修复——新增 `validate_shelf_name`（ASCII 字母数字 + `-` `_`，≤64 字符），`create_shelf` 与 `create_offline_shelf` 统一入口校验，CLI 与 Web 同时生效。

`main.rs create_shelf` 与 web 层货架创建此前对货架名无字符校验，与库名（ASCII 字母数字 `- _`，且 canonical 一致性校验）、磁带名（禁 `<>"/\:|*?` 与控制字符）的白名单策略不一致。货架名会进入：错误消息（见 L14）、Web API JSON、SQLite、`operation_events` 日志。修复后货架名白名单与库名对齐。

### L16（提示级）：`/api/setup/complete` 存在 TOCTOU 小窗口

`try_complete_primary_vtl_setup_from_web`（`main.rs:2827`）的"主配置已存在则拒绝"检查与 `write_initial_vtl_conf` 写文件之间无锁；两个并发已认证请求可同时通过检查，后写覆盖先写。影响小（仅初始化窗口、已认证管理员、路径均经 `normalize_setup_path` 绝对路径校验），记录不改码；如需消除可用临时锁文件或 `create_new` 语义写文件。

### M7 复核确认（维持原结论）

`web.rs api_login` 错误日志将 `body.username`（trim 后）直接插入 `log_error`（约 1189/1238 行），即第二轮 M7 描述的控制字符注入日志通道。利用需先通过 JSON 解析（原始字节 NUL 无法入 String），实际风险为日志混淆级，维持"提示级"处置建议（写入前过滤控制字符）。

---

## 二、此前未逐行审查面的审查结论

### 1. Vue3 前端（`userspace/web`，~2300 行）——通过，无新发现

- **CSRF**：所有变更请求经 `client.ts` 统一注入 `X-VTL-CSRF`（从 cookie 读取，与后端双提交校验匹配）；遗留 `web_boot.js` 的 fetch 包装对非 `/api/login` 的 POST 同样补头。
- **路由守卫**：`router.beforeEach` 对非 public 页面先 `checkSession()`，强制改密用户重定向 `/account`；`/login`、`/setup` 标记 public。
- **渲染安全**：全部数据走 naive-ui 组件 prop 插值（Vue 默认转义），未发现 `v-html`。
- **开放重定向**：登录后 `router.replace(route.query.redirect)` 走 vue-router 内部导航（含协议字符串按 path 解析），不构成外部开放重定向。
- **无密钥/凭据**：前端无任何 token、密码硬编码；登录只提交密码 + 一次性验证码答案。

### 2. web.rs 认证中间件与管理 API ——通过（发现见 L14）

- 会话 cookie `HttpOnly; SameSite=Lax`，Secure 跟随 `VTLADM_WEB_COOKIE_SECURE`；CSRF cookie 双提交校验；POST 全量过 CSRF 检查（`/api/logout` 除外，注销无敏感面）。
- 登录限流 key 仅当对端为回环时才信任 `X-Forwarded-For`，反代欺骗面收敛。
- 强制改密门、setup 门均在中间件层强制执行，API 返回结构化错误码（`must_change_password` / `setup_required`）。
- `/api/setup/complete` 未列入 public 路由——初始化向导必须先以初始密码登录，服务端再验 `setup_gate_active()` + 配置存在性（幂等拒绝重复初始化）；`run_kernel_reload_now` 默认 false，避免保存配置即触发 rmmod/insmod。
- iSCSI 系列 API（quick-export/unexport、library-export/unexport）：argv 形式执行 `vtladm-iscsi`（无 shell 插值），IQN/fileio 名/portal 白名单校验，`ensure_tape_file_under_tape_dir` canonicalize 后强制落在 tape_dir 之下，非 dry-run 需显式 `allow_iscsi_exec`。
- 管理 API 统一模式：`require_session` → `spawn_blocking` → 错误字符串化返回；`LibraryGuard` 切换 thread-local 当前库并在 Drop 恢复，`spawn_blocking` 每任务独立线程，无跨请求串库。

### 3. 内核同步/对账/监控模块（robot_sync / reconcile / monitor / label / scsi_rescan_vtl）——通过

- **ioctl 契约**：`robot_sync.rs` 自带测试断言 ioctl 号与 `_IOW/_IOWR` 宏计算一致（防结构体漂移）；固定缓冲全部先填 0 再做长度检查（name < 64、barcode < 16）。
- **inventory 安全**：`VTL_INV_MAX_ITEMS=128` 截断时拒绝 sync/reconcile（`ensure_inventory_complete`）；DB/内核双侧重复占用均显式报错；`apply_kernel_to_db` 空内核不动 DB（防误清全部分配），事务提交，含回归测试（SQL `?1` 复用绑定数）。
- **状态持久化**：changer state 写走 tmp + rename 原子替换。
- **monitor**：仅读 `/proc`，SQL 全参数化，快照/事件表自动清理 90 天。
- **label**：定长 80 字节标签缓冲，space-pad 截断，写 VTL 块头（magic+大小+algo=NONE），`set_len(0)` 失败（zvol 块设备）时按覆盖语义继续——注释已说明。
- **scsi_rescan**：仅写 `proc_name == "vtl"` 的 host scan，host 序号数值排序，stagger 可配，含临时目录单测。

### 4. main.rs 剩余函数面 ——通过

- **名称/路径**：`validate_library_name` 白名单 + canonical 一致性双校验；`validate_tape_name` 禁路径分隔符/通配符/`<>`/控制字符；`sanitize_lib_dir_component` 兜底；`parse_size` f64 上限 + `u64::MAX` 双检查。
- **`link_kernel_tapes`**：canonicalize + inode 比对防 symlink 欺骗，重名冲突计数报错，根目录孤儿实体文件搬迁 `_orphaned_root`（不删除），失效 symlink 清理。
- **`delete_named_library`**：事务删库（tapes→tags→drives/slots→shelves→library），保护最后一个在线库，文件删除在提交后并收集 warning 不中断；删当前库后自动切换 `CURRENT_LIBRARY` 到下一在线库。
- **`CURRENT_LIBRARY`** 为 `thread_local!`，Web 并发请求经 `LibraryGuard` 互不影响（已确认非全局可变状态）。
- **ioctl 门**：`refuse_ioctl_before_hot_geom` / `refuse_ioctl_if_vtl_holders_busy` 在几何变更/holder 占用期间拒绝危险操作并给出操作指引。

### 5. 依赖与构建（Cargo.toml / Cargo.lock）

关键依赖精确 pin（`clap =4.0.32`、`sha2 =0.10.8`、`uuid =1.8.0`、`time =0.3.36`），注释说明 pin 原因（edition2024 传递依赖对旧发行版 Cargo 的兼容性）；`Cargo.lock` 入库可复现构建；`build.rs` 强制 Linux + 64 位。无已知漏洞版本的依赖钉死，无 git 依赖。

### 6. packaging / scripts 复核

`vtl-kernelctl`、`vtl-link-kernel-tapes`、`vtladm-web-serve` 及 scripts/ 全部脚本：无 `rm -rf`（除第一轮已确认限定 `$PREFIX` 的 install/uninstall）、无 `eval`、无 `curl|bash`；命令替换均为固定命令；systemd unit 与 udev 规则无异常。与第二轮结论一致。

---

## 三、既有修复复核（抽样）

对 `03ab310` 基线抽样复核前两轮修复仍然完好：M4 sidecar 校验块在装载路径就位；M6 zpool 显式报错就位；L1 `memset` 在 `copy_from_user` 之前；L3 压缩侧死代码已删、解压路径保留；L13 备份 helper 覆盖两个写盘点。工作区干净，无回归迹象（静态层面）。

---

## 四、三轮覆盖度矩阵（完整审查声明）

| 代码面 | 第一轮 | 第二轮 | 第三轮 | 状态 |
|--------|--------|--------|--------|------|
| 内核 SCSI handlers / ioctl / tape 路径 | 重点精读 | 全 handler | 复核 | 已覆盖 |
| 内核生命周期 / 热改几何 | 重点精读 | ✓ | — | 已覆盖 |
| kernel 压缩引擎 | 死代码标注 | — | 修正后复核 | 已覆盖 |
| main.rs（8450 行） | 风险模式+重点函数 | 关键函数 | 剩余函数全扫 | 已覆盖 |
| web.rs（7354 行） | 风险模式 | 认证/硬编码 | 管理 API+遗留页全扫 | 已覆盖 |
| web_auth.rs | — | 硬编码审查 | 复核 | 已覆盖 |
| patrol/lio_hold/scsi_tape_holders/zvol/iscsi | — | ✓ | 复核 | 已覆盖 |
| robot_sync/reconcile/monitor/label/scsi_rescan | — | — | 逐行 | 已覆盖 |
| Vue3 前端（2300 行） | — | 未审 | 逐文件 | 已覆盖 |
| 脚本与打包 | 规范扫描 | 风险模式 | 复核 | 已覆盖 |
| 硬编码专项 | — | 五类扫描 | — | 无生产路径硬编码 |

---

## 五、全部遗留问题汇总（截至第三轮）

| 条目 | 级别 | 说明 | 建议 |
|------|------|------|------|
| M5 | 中（文档项） | Web 会话内存态、无审计落盘 | 文档先行设计 |
| M7 | 提示（已修复 `b8eccb6`） | ~~登录错误日志插入未消毒的用户名~~ | `sanitize_for_log` 已落实 |
| M8 | 提示（已修复 `b8eccb6`） | ~~RESIZE_GEOMETRY int 级瞬时不一致~~ | 注释已落实 |
| L4 | 低（已修复 `b8eccb6`） | ~~filemark 读返回报 BLANK CHECK 而非 FILEMARK sense~~ | FILEMARK sense 已落实 |
| L5 | 低（已修复 `b8eccb6`） | ~~move_medium ref 时序依赖隐式不变式~~ | 先 pin 后 put 已落实 |
| L14 | 低（已修复 `59871db`） | ~~遗留页 `j.error` 未转义入 innerHTML~~ | 统一 escapeHtml，已落实 |
| L15 | 低（已修复 `59871db`） | ~~货架名无字符白名单~~ | 与库名对齐，已落实 |
| L16 | 提示 | setup complete TOCTOU 小窗口 | 可不改 |

高危（H1–H3）与全部可修中低危项（M1–M4、M6–M11、L1–L3、L5–L8、L13–L15）已闭环；遗留仅剩 M5（文档先行项）与 L16（提示级，记录未改码）。

---

## 六、审查限制声明

- 未编译、未运行：所有结论基于源码静态分析；本轮新发现（L14 渲染链路）建议在 Linux 构建机以浏览器实测确认修复效果。
- 前端依赖（naive-ui 等 npm 包）的供应链未做漏洞扫描，仅审查了本项目源码。
- M9/M10 的触发依赖具体备份软件行为矩阵，仍建议结合 TEST.md 在真实备份软件下回归。
