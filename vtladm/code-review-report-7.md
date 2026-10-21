# VTLADM 源码审查报告（第七轮）

- 日期：2026-10-21
- 基线提交：`e2620a6`（第六轮修复合入后）
- 分支：`vtladm26`
- 范围：全量通读内核 5 个核心文件（vtl_scsi.c 2460 行、vtl_tape.c 2244 行、vtl_main.c 1551 行、vtl_misc.c 433 行、vtl_state_restore.c / compression.c / vtl_sysfs.c），userspace 重点模块（web_auth.rs 全文、web.rs 安全中间件与路由、robot_sync.rs、main.rs 常量区与生产路径、zvol.rs 全文）+ 全仓模式扫描（硬编码路径、unwrap/unsafe、未检查返回值）
- 结论：**第六轮修复（F1/R1/R2/W1/W2/W3/W5/S1/S2）全部确认到位且上下文正确**。新发现 3 个低中风险问题、若干信息项；无高危问题。M5（Web 会话持久化）代码侧已完整落地，无缺口。

---

## 1. 第六轮修复回归验证（逐项亲验）

| 项 | 验证结果 |
|---|---|
| F1 EOD sense | `vtl_handle_read`（vtl_scsi.c:1126-1136）BLANK_CHECK + byte0 VALID + byte2 EOM + bytes3-6 残余量；`blocks/block_len` 在作用域内，`resid` 上界 64 MB 无 u32 溢出；`vtl_build_sense_buffer` 先清零再按位或，不会叠加脏位。✔ |
| R2 防抽带闸门 | `vtl_misc.c` 四个 ioctl（load/unload/move/elem_remove）均先取实例（io_sem 读锁）再闸门再操作，`put_instance` 配对完整；`vtl_move_endpoints_busy` 两端检查；userspace `evacuate_element` 对 EBUSY 降级为"本轮跳过"。SCSI MOVE MEDIUM CDB 路径未闸门——注释明确记载为既定决策。✔ |
| W1 errno 保真 | `vtl_rec_flush` 数据区 `return ret`（vtl_tape.c:1154）；单块直写 `ret<0` 分支保 errno（1294-1295）；scsi 层 -ENOSPC→VOLUME_OVERFLOW 映射已亲验存在（vtl_scsi.c:1189-1190）。✔（头写入路径见 N3，不一致残留） |
| W2 实际长度 | 两处 `*actual = to_write`。✔ |
| W3 erase 不落盘 | erase 处理器改内存丢弃暂存块，语义正确（rec_buf 内存保留至 unload，无泄漏）。✔ |
| W5 meta 持久化 | 全部 9 处 `vtl_meta_write` 调用点逐一核对：除 init-tape（vtl_tape.c:283，注释声明 non-fatal）和模块卸载收尾（:520，进程已死无需告警）外均已包 `<0` 检查。✔ |
| R1 读游标复位 | helper 定义一处，调用六处（load/unload/space/erase/locate/write）全部亲验；rewind 用直接赋值（语义相同，风格不统一但无害）。✔ |
| S1 日志限流 | queuecommand 错误路径 `pr_info_ratelimited`。✔ |
| S2 vtlfm 容忍 | 零偏移 `continue` 不推进 `prev`，严格递增检查不被削弱；`pr_warn_ratelimited`。✔ |

## 2. 新发现

### N1【中】`vtl_changer_remove_medium` 驱动器分支缺失 `vtl_rec_flush` — 静默数据丢失窗口

`kernel/src/vtl_tape.c:1837-1858`。驱动器分支在卸载前直接持久化 metadata 并清空 `loaded_tape`，**没有先 flush 驱动器里的暂存写记录**。对比 `vtl_changer_move_medium` 驱动器源分支（:1587）有 flush。

后果链：initiator 已收到 GOOD 的写还暂存在 64KB rec_buf 里 → 备份空闲超过 R2 闸门 120 秒 → sync 撤离（`ELEM_REMOVE`，即 10-21 事故走的路径）或 web UI 下架 → 暂存记录随驱动器清空而消失，且 `meta.used` 持久化的值不含这些记录（表面一致，实际丢数据）。R2 闸门只覆盖"最近 120 秒有 I/O"的窗口，不能替代 unload 前 flush。

修复建议：在 `vtl_changer_remove_medium` 驱动器分支、`mutex_lock(&d->lock)` 之后，参照 move_medium 增加：
```c
if (vtl_rec_flush(d, t, false) < 0)
    pr_warn("VTL: remove_medium: flush failed for %s — staged writes may be lost\n", t->name);
vtl_read_cursor_reset(d);
```

### N2【中低】`vtl_changer_move_medium` 驱动器源分支 flush 返回值被忽略

`kernel/src/vtl_tape.c:1587`。`vtl_rec_flush(src_drv, t, false)` 的返回值未检查：flush 失败（如 backing fs 写满）时暂存记录被丢弃（retain=false），而 move 继续并返回成功，mtx/web 看到"成功卸载"但数据已丢。至少应 `pr_warn`；更稳妥的做法是 flush 失败时中止 move（goto rollback 会把磁带放回驱动器，暂存数据仍在）。

### N3【低】`vtl_rec_flush` VLBK 头部写入失败仍折叠 errno

`kernel/src/vtl_tape.c:1130-1139`。头部 8 字节写失败路径 `return ret < 0 ? -EIO : -ENOSPC;` 与 W1 修复过的数据区路径（`return ret;`）不一致：-ENOSPC 在这里被折成 -EIO，scsi 层会报 MEDIUM ERROR 而不是 VOLUME_OVERFLOW。头部写失败概率极低（同 fs 上数据区已写成功的情况几乎不存在），但语义应统一为 `return ret < 0 ? ret : -ENOSPC;`。

### N4【低】用户主动 load/unload/move 遇 R2 闸门 EBUSY 仅 warn，不 surfaced 给调用方

`userspace/src/main.rs:4985, 5086, 5167`。这三处是 web/CLI 发起的交互式操作，被闸门拒绝时只打 warn，用户看到的是"操作成功"但内核侧未执行，依赖下一轮 5 分钟 sync 收敛。建议对 EBUSY 特判：向前端返回明确错误（"驱动器有近期 I/O，请稍后重试"），避免操作结果错觉。不影响数据安全，列为体验/一致性项。

### N5【低】`vtl_changer_prepare_shrink` 与并发 move 之间存在窄竞态

`kernel/src/vtl_main.c:682-710`。该函数先逐个检查待移除驱动器（不持 ch->lock），再持 ch->lock 检查槽位。与并发 `move_medium`（全程持 ch->lock）之间存在窗口：检查后、resize 生效前装入"待移除驱动器"的介质会滞留在几何上不可见的驱动器里（静态数组，无 OOB，介质可经 GET_INVENTORY 找回）。resize 是管理员显式操作且与 reconfig 互斥，实际触发概率低；若要堵死，可在 resize 持 ch->lock 期间复查驱动器占用。

### N6【信息】杂项

1. `vtl_main.c:1235` 注释 "deferred ~60s" 与代码 `10 * HZ` 不符（应为 ~10s）。
2. `vtl_tape.c:2024` `char barcode[sizeof(drv->loaded_tape->meta.barcode)];` — sizeof 不求值无运行时解引用，但写法易误读，建议写常量 16。
3. `vtl_changer_move_medium` 装载目标驱动器时在 ch->lock + drv->lock 内做 `filp_close/filp_open`（:1663-1666）——本地文件可接受；若 backing store 放在网络文件系统上会放大锁持有时间。当前部署形态（本地文件/zvol）无实际影响。
4. `vtl_changer_move_medium` rollback 的 storage 分支 `kref_get(&t->ref)` 先于 `occupied = (t != NULL)` 判空（:1764-1766）——rollback 仅在 extract 成功后可达，t 恒非 NULL，无实际风险，仅防御顺序不佳。
5. sysfs `create_tape` store（vtl_sysfs.c）无 CAP_SYS_ADMIN 检查——sysfs 属性 mode 0200 仅 root 可写，且路径经 `vtl_tape_name_valid` 过滤，风险可接受；如求一致可补 capable() 检查。

## 3. 硬编码专项审计（延续第六轮关注点）

全仓扫描 `/opt`、`/etc`、`/dev`、`/var`、内网地址字面量：

| 位置 | 内容 | 结论 |
|---|---|---|
| main.rs:2636-2646 | `VTL_INSTALL_PREFIX` / `PRIMARY_VTL_*` 常量区集中定义安装路径 | 良好——单一事实源 |
| vtl_tape.c:74 / vtl_main.c:1505 / vtl_state_restore.c:24 | 内核默认 tape_dir/state_dir | 均为 module_param，可覆盖；userspace 与内核常量值一致 |
| web.rs:450 | `VTLADM_WEB_UI` env + `/opt/vtladm/bin/vtladm` fallback | env 优先，可接受 |
| patrol.rs:629 | `VTLADM_WEB_HOST` env + `127.0.0.1` fallback | 可覆盖 |
| vtladm_iscsi.rs:683-684 | `/etc/tgt/targets.conf(.vtladm-bak)` | tgt 守护进程规范路径，且有"首次覆盖前备份"保护（L13），合理 |
| zvol.rs:54/159 | `/dev/zvol/...` | ZFS 内核约定路径，合理 |
| kernel_geom_ioctl.rs:35 / robot_sync.rs:237 / patrol.rs:158 | `/dev/vtl` | 内核 misc 设备约定名，合理 |
| main.rs:6710-6712 等 | `/var/tmp/vtladm_utest/*` | 单测路径，合理 |

**结论：无散落的魔法路径/地址。** 安装前缀有常量区收口，运行参数有 module_param / 环境变量覆盖。

## 4. M5（Web 审计/会话持久化）代码核验

此前标记为"文档先行项"的 M5 已在代码侧完整落地（`web_auth.rs` + `web.rs`），本轮逐行核验：

- 会话 SQLite 落盘（`web_sessions` 表），60 秒节流回写 `last_activity` 防写放大；进程重启 `restore_sessions()` 恢复未过期会话；淘汰/登出/改密同步删库；启动时清理过期行。✔
- 令牌明文落库的风险注释明确（catalog 库 0600），可接受。✔
- 初始随机 UUID 密码 + 强制改密门（`password_change_required` 时仅放行账户页/改密/登出）。✔
- 认证文件 tmp+rename 原子写、0600 权限。✔
- 登录：bcrypt、算术验证码一次性、IP+用户名限流（5 次/60s，锁 300s）、日志消毒（M7）。✔
- CSRF：全站 POST 强制 `x-vtl-csrf`（logout 豁免）；**全仓无 PUT/DELETE/PATCH 路由**，不存在方法绕过面。✔
- Cookie：session cookie HttpOnly + SameSite=Lax + Secure 默认开（`VTLADM_WEB_COOKIE_SECURE` 可关，适配纯 HTTP 内网部署）。✔
- 生产代码 unwrap 审计：main.rs 非测试 unwrap 仅 2 处且均有前置守卫（file_name() 前有 is_file 判断；existing_tape.unwrap() 在 is_none() 的 else 分支）；web_auth.rs 的 unwrap 均为 Mutex::lock（毒化即 panic，守护进程惯例，可接受）；reconcile/monitor/web.rs 的 unwrap 集中在 `#[cfg(test)]`。✔

## 5. 建议修复顺序

1. N1（remove_medium 补 flush）—— 与 R2 闸门互补，堵上最后的静默丢数据窗口，**建议随下一轮一并修复**。
2. N2（move 源分支 flush 返回值）—— 同文件邻近位置，改动两行。
3. N3（rec_flush 头部 errno）—— 一行改动，与 W1 收口。
4. N4（EBUSY surfaced 给前端）—— userspace 体验项，可单独排期。
5. N5/N6 —— 记录备查，不急于动。

## 6. 总体评价

第六轮修复质量良好：F1 的 sense 构造经 st.c 4.19/6.6 源码逐行验证，R2 闸门覆盖了全部四个元素变更 ioctl 且 userspace 优雅降级，W/R/S 系列在数据路径上闭合。内核锁序（ch→drv→tape）全仓一致，kref 生命周期（L5/H2）在 move、ioctl、teardown 三条路径上闭合，元素地址 0 别名歧义在各分支均有负索引护栏。本轮新发现的问题集中在**卸载/撤离路径上暂存写记录的 flush 完整性**（N1/N2/N3 是同一主题的三个面），建议作为下一修复轮的唯一主题。
