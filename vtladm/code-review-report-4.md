# vtladm 虚拟磁带库（VTL）项目代码审查报告（第四轮 · M5 后回归审查）

- 审查日期：2026-10-13（第四轮，接续 `code-review-report.md`、`-2.md`、`-3.md`）
- 审查范围：M5 提交（`632bbdf` 会话持久化 + 认证审计）的对抗性复审、全部既有修复的完好性复核、薄覆盖模块精读（zvol / kernel_geom_ioctl / lio_hold / fab_transport / test_utils / iscsi_export / vtladm_iscsi 剩余段）、全库注入面与内核 C 面模式扫描。
- 审查方式：纯静态审查。本机 Windows，目标 Linux，未编译未运行。
- 代码基线：分支 `vtladm26`，提交 `8dca67e`。

---

## 一、本轮新发现

### M12（中危，M5 引入）：认证审计在 setup 门前落库，`init_db()` 会创建 catalog 库

**链路**：`api_login` 是 public 路由（setup 完成前可达）。M5 给每次验证码失败 / 密码错误接上了 `super::monitor::log_event("auth", ...)`，而 `log_event` 内部调用 `crate::init_db()`（`monitor.rs:12` `use crate::init_db`）。`init_db()`（`main.rs:3336`）的行为是 `create_dir_all(parent)` + `Connection::open`（**文件不存在则创建**）+ 建全套 schema + 迁移检查。

**后果**：setup 门（`setup_gate_active()`，`main.rs:2699`）只看 `vtl.conf` 是否存在，不看 db。因此**未认证用户**在 setup 完成前反复 POST `/api/login`（错误验证码 / 错误密码每次都会触发），即可让服务端创建 `/opt/vtladm/var/vtl.db` 及目录树。属于未认证请求引发的服务器文件系统写入。setup 向导本身不校验 db 是否存在（`main.rs:2835` 只拒绝 vtl.conf 已存在），多数场景下最终可用，但存在两个实质问题：

1. 若向导选择了**非默认 db_path**，默认路径会留下一份带 schema 的孤儿库文件；
2. 该副作用绕过了 M5 中 `SessionStore::open()` "web 层不得提前创建 catalog 库" 的设计意图——审计路径自己把库建出来了（运行时 `session_store` 仍是 None，行为不自洽）。

**修复建议**：`log_event` 落库前检查 db 文件存在性（不存在则跳过，stderr 日志不受影响）；或在 `api_login` 审计调用处加 `!super::setup_gate_active()` 门。推荐前者——一次性把"事件路径永不建库"变成全局不变式。

### M13（低危，M5 引入）：审计/日志在 async handler 内同步执行重 IO，登录风暴可放大为 DoS

`api_login` 每次失败分支同步调用 `log_event`，而 `log_event` 每次调用都 `init_db()`：**新建 SQLite 连接 + PRAGMA + 多张表的 table_exists/column_exists 检查 + 迁移分支**，`busy_timeout` 8 秒，全部在 axum async 上下文内联执行（未走 `spawn_blocking`）。每次错误密码/验证码尝试都触发一次。攻击者以高频率打登录端点即可让 tokio worker 长时间阻塞在 SQLite 打开/探测上，web UI 可用性下降。

注：`log_message`/`log_error` 写日志文件同属内联阻塞，但频率低得多；M5 把重 IO 的调用频率放大到了"每次登录尝试"。

**修复建议**：给 monitor 加进程级连接缓存（或 `OnceLock<Mutex<Connection>>`），`log_event` 复用连接；或在 `api_login` 审计点包 `spawn_blocking`。

### L17（提示级，M5 引入）：`web_sessions` 残留行仅靠 restore 时清理

`session_username` 过期分支会同步删库，但 `session_is_valid`（ping 路径，刻意不改语义）过期时只删内存。长跑进程（不重启）中这些行会残留到下次 `load_active`。单行成本极小、量有界（登录速率 × 24h），可接受；如要消除，在 ping 过期分支补一次 `store.remove` 即可（会引入写放大，权衡后建议保持现状并加注释）。

### L18（提示级，M5 引入）：`SessionStore::open` 不校验文件是有效 SQLite

db 路径存在但不是有效库（损坏 / 非 db 文件）时，`Connection::open` 成功（惰性），`ensure_schema` 静默失败，`session_store` 为 Some 但所有写都吞错——持久化悄悄失效且无告警。建议在 `open()` 时做一次只读探测（如 `PRAGMA schema_version` 或 `SELECT count(*) FROM sqlite_master`），失败返回 None 降级内存并在 stderr 提示。

### L19（提示级，M5 引入）：setup 前启动的 web 进程持久化永不生效

`session_store` 只在 `run_web_ui` 启动时打开一次。web 先于 setup 启动 → db 不存在 → None；setup 完成后**不重启 web 则会话一直纯内存**（审计 `log_event` 不受此限，M12 反而会把库建出来）。systemd 单元若按 install.sh 顺序启动则无此问题；建议文档注明，或 setup complete 成功后提示重启 web 服务。

---

## 二、既有修复复核（全部完好）

| 修复 | 提交 | 复核位置 | 状态 |
|------|------|----------|------|
| H1 MOVE MEDIUM filp_open 误报成功 | `04b92ec` | `vtl_tape.c:1634-1642`（PTR_ERR 先存再清 t->file） | ✓ |
| H2 ioctl 期间 changer 生命周期 | `7673448` | `vtl_main.c:61`（io_sem read lock 注释在位） | ✓ |
| H3 Unit Attention cmd->result | `04b92ec` | `vtl_scsi.c:2329-2335`（显式设 result） | ✓ |
| L1 inventory 内存泄漏 | `03ab310` | `vtl_misc.c:115`（copy_from_user 前 memset） | ✓ |
| L4 filemark 读 sense | `b8eccb6` | `vtl_scsi.c:1096-1103`（FILEMARK sense bit） | ✓ |
| L5 pin-before-put | `b8eccb6` | `vtl_tape.c:1526/1550/1585`（三分支 kref_get 先于引用释放） | ✓ |
| M4 sidecar 偏移校验 | `0cdc749` | `vtl_tape.c:635-656`（严格递增 + ≤ EOD） | ✓ |
| L14/L15/M7/M8 | `59871db`/`b8eccb6` | 前轮已确认，本轮未见回归 | ✓ |

`move_medium` 额外核对：三个 extract 分支均"先 kref_get 再释放元素引用"，成功放置分支再取元素引用，rollback 三分支对称恢复，`out:` 释放函数级 pin——引用计数对称；`msleep` 在 `ch->lock` 释放之后（`vtl_tape.c:1764-1766`），无持锁睡眠。inventory ioctl：固定缓冲 kvmalloc、先 memset 后 copy_from_user、copy_to_user 前 count 双上限。misc 设备 `mode = 0600` + 全部 ioctl `CAP_SYS_ADMIN` + `compat_ioctl` 同handler（纯定长结构，无指针成员，compat 安全）。

---

## 三、本轮扫描无发现的面

- **SQL 注入**：全库无 `format!` 拼接 SQL，rusqlite 调用全参数化（`params!`）。✓
- **命令注入**：33 处 `Command::new` 全部 argv 形式，无 `sh -c` 插值用户输入；唯一 `sh -c`（`lio_hold.rs:56`）为固定字符串 + `timeout 20`。web.rs 阻塞命令全部包 `spawn_blocking`。✓
- **zvol.rs**：argv 传参无 shell 面；zfs 路径总是以池名开头，无 `-` 开头选项注入；`zfs destroy -f` 删除前 `zvol_exists` 确认且池/库/名均经白名单校验链。`sync=disabled` 已注释说明（靠带标签做完整性）。`zvol_zero_first_block` 为 dead code，内部 `vec![0u8; block_size]` 分配无上界——未在生产路径，记录不改。
- **kernel_geom_ioctl.rs**：ioctl 号与内核 `_IOW` 宏有单测对拍；spec 长度上限校验；结构体零填充后拷贝。✓
- **lio_hold.rs**：configfs 遍历有界（树小），读文件失败静默属合理（探测型）。✓
- **fab_transport.rs / test_utils.rs**：纯展示与环境准备；测试用 `set_var` 在 edition 2018 下安全；`VTL_SKIP_KERNEL_RELOAD=1` 防测试触发内核重载，注释充分。✓
- **iscsi_export.rs**：全参数化 SQL，JSON 列 serde 解析，portal_port `u16::try_from` 收窄校验，FK `ON DELETE CASCADE`。
- **vtladm_iscsi.rs**（剩余段）：tgt 配置重写从 `tgtadm show` 解析，IQN 经 `validate_iqn` 白名单；lun-map 上限 255 + 重复检测；quick-export 校验 regular file + 非空 + `canonicalize`。

---

## 四、遗留问题汇总（截至第四轮）

| 条目 | 级别 | 说明 | 建议 |
|------|------|------|------|
| M12 | 中（已修复 `2c6b8fc`） | ~~审计 `log_event` 在 setup 门前经 `init_db()` 创建 catalog 库~~ | db 存在性门已落实（事件路径永不建库） |
| M13 | 低（已修复 `2c6b8fc`） | ~~审计重 IO 内联于 async handler，每次登录尝试触发~~ | 进程级连接缓存已落实（测试绕过） |
| L16 | 提示 | `/api/setup/complete` TOCTOU 小窗口 | 维持记录不改（前轮结论） |
| L17 | 提示（M5 引入） | ping 路径过期行残留至 restart | 保持现状，加注释 |
| L18 | 提示（M5 引入） | `SessionStore::open` 不探测有效 SQLite | 打开时 `PRAGMA schema_version` 探测 |
| L19 | 提示（M5 引入） | setup 前启动的 web 持久化不生效 | 文档注明 / setup 后重启 web |

M1–M11 已闭环（M5 于 `632bbdf` 修复并标记于第三轮报告），L1–L15 已闭环，M12/M13 于 `2c6b8fc` 修复。H1–H3 复核完好。

---

## 五、审查限制声明

本机 Windows，无 cargo/内核构建环境；本轮全部结论来自静态审查，M12/M13 为确定的行为链（代码路径已逐段核实），但未做运行复现。合并前请在 Linux 执行 `cargo test` 与一次 setup 向导 + 登录失败的端到端验证（重点观察 setup 完成前 `/opt/vtladm/var/vtl.db` 是否被创建，以实证 M12）。
