# vtladm 虚拟磁带库（VTL）项目代码审查报告（第五轮 · Kylin st-offline 误判归因更正后的防御机制审查）

- 审查日期：2026-10-20（第五轮，接续 `code-review-report.md`、`-2.md`、`-3.md`、`-4.md`）
- 审查范围：确认"Kylin st offline 级联"为误判（真实根因 = 安装/卸载脚本在 `/dev/st*|sg*|sch*` 被占用时强制 rmmod，已由 `vtl-kernel-safe.sh:vtl_preflight_before_rmmod()` 阻断）之后，对当年基于该误判引入的全部内核防御机制做逐条复审，决定裁剪/保留，并核查是否仍有同类误判归因残留。
- 审查方式：纯静态审查。本机 Windows，目标 Linux，未编译未运行。
- 代码基线：分支 `vtladm26`，提交 `e82d1cd`。
- 结论先行：**1 处建议裁剪（K1），1 处建议合并（K2），4 处保留（K3–K6），无高危问题。**

---

## 一、机制清单与裁定总表

| # | 机制 | 位置 | 裁定 |
|---|------|------|------|
| K1 | queuecommand 每命令自动恢复 SDEV_RUNNING | `vtl_scsi.c:2327-2334` | **裁剪**（中危） |
| K2 | MOVE MEDIUM 成功后恢复驱动器 online | `vtl_scsi.c:1615-1621` | 合并进 K3 或保留（低危） |
| K3 | offline_guard_work（30s 周期） | `vtl_main.c:1067-1092` | 保留（唯一兜底） |
| K4 | TUR 对空带机恒 GOOD | `vtl_scsi.c:393-418` | 保留 |
| K5 | EH 三回调恒 SUCCESS | `vtl_main.c:1532+` | 保留 |
| K6 | install.sh st warmup | `install.sh:711-722` | 保留 |
| K7 | MOVE MEDIUM sense 按 errno 映射 | `vtl_scsi.c:1587-1607` | 已修复（`c1247e8`），复核通过 |

---

## 二、K1（中危，建议裁剪）：queuecommand 热路径里的设备状态改写

**代码**（`vtl_scsi.c:2327-2334`）：

```c
int vtl_scsi_queuecommand(struct Scsi_Host *shost, struct scsi_cmnd *cmd)
{
    /* Auto-restore VTL devices that Kylin st driver offlines */
    {
        struct scsi_device *sdev;
        shost_for_each_device(sdev, shost) {
            if (sdev->sdev_state == SDEV_OFFLINE)
                scsi_device_set_state(sdev, SDEV_RUNNING);
        }
    }
    ...
```

**四个问题**：

1. **运行上下文错误**。SCSI 中层在 `scsi_dispatch_cmd()` 内持 `shost->host_lock`（spin_lock_irqsave，关中断）调用 `queuecommand`——这在 4.18–6.10 整个支持窗口内成立（host_lock 从派发路径移除是 ~6.11 之后的上游改动）。也就是说这段代码在**每次 I/O 的关中断自旋锁区间**里遍历设备列表。当前能跑只是因为 `shost_for_each_device` + `scsi_device_set_state`（内部 `sdev_evt_send_simple(..., GFP_ATOMIC)`）恰好都不睡眠；这是踩线正确，不是设计正确——任何后续维护者在这里加一个可能睡眠的调用就是 "sleeping in atomic"。
2. **绕过 state_mutex 的裸写竞态**。`sdev->sdev_state` 的合法改写方都持有 `state_mutex`（sysfs `state` store、`scsi_set_device_state`）。LLDD 在这里裸读裸写：st 的错误恢复路径正把设备离线时，queuecommand 可以把它同时拉回 RUNNING，双方对打；`scsi_device_set_state` 的状态合法性检查返回值（如 -EINVAL）被无视。
3. **纯热路径开销**。每条 SCSI 命令（含备份窗口内全速的磁带读写）都遍历 host 下全部 LUN（最多 1 changer + 8 tape = 9 个设备），仅为处理一个原本 30 秒才需要检查一次的状态。
4. **注释仍是旧误判**。`Auto-restore VTL devices that Kylin st driver offlines`——归因未更正（`e82d1cd` 只改了别处）。

**裁定**：删除 queuecommand 入口的这段。它的存在理由是"st 会离线设备"，该前提已证伪；其功能与 K3 完全重复，且 K3 的上下文（workqueue、可睡眠）才是合法位置。

## 三、K2（低危）：MOVE MEDIUM 后的 online 恢复——建议合并

`vtl_scsi.c:1615-1621`：MOVE MEDIUM 成功后在同一 queuecommand 上下文恢复所有离线驱动器。

- 频率低（机械手命令，非数据面），开销可忽略；
- 触发时机确实有价值：insmod / 大批量 MOVE 之后正是 st 轮询、udev 探测的密集窗口；
- 但同样存在 K1 的 2（state_mutex 竞态）问题。

**裁定**：裁剪 K1 后，此处二选一：
(a) 保留（事件驱动、量极小，竞态面远小于 K1）；或
(b) 改为 `mod_delayed_work(&vhost->offline_guard_work, 0)`，让恢复统一走 workqueue 路径——语义相同、上下文合法、机制收敛到一处。推荐 (b)。

## 四、K3（保留）：offline_guard_work 是唯一合法兜底

`vtl_main.c:1067-1092`：delayed work，30s 周期，`shost_for_each_device` + 恢复 RUNNING。

- workqueue 上下文可睡眠，与中层改状态的锁序无冲突面；
- 卸载路径 `cancel_delayed_work_sync()`（`vtl_main.c:354,1278`）干净；
- 注释已在 `e82d1cd` 更正。

**裁定**：保留，作为唯一兜底。注意它同样有 K1 问题 2（裸写 state），但 30s 一次 vs 每条 I/O 的竞态概率天差地别，且裁剪 K1 后它承担全部职责——如要彻底干净，可改为 `scsi_set_device_state` 不可行时至少补一句注释说明此处刻意绕过 state_mutex 的取舍。

## 五、K4（保留）：TUR 对空带机恒 GOOD

`vtl_scsi.c:393-418`。注释（`e82d1cd` 后）给出的理由：mhVTL 兼容行为 + 备份软件轮询预期。本次复核补充验证：注释声称"backup apps get proper errors when accessing an empty drive"**属实**——空带机 READ 走 `vtl_tape_read` 返回 `-ENODEV`，handler 转 NOT READY 3A/00（`vtl_scsi.c:1086-1091`）；WRITE 同理。只有 TUR/REWIND/SPACE/LOAD_UNLOAD 这类机械命令对空带机回 GOOD。

**裁定**：保留。改为 SSC 的 NOT_READY 会改变备份软件看到的行为（驱动器探测逻辑），收益不明、风险实在，不建议动。

## 六、K5（保留）：EH 三回调恒 SUCCESS

`vtl_main.c:1532+`。mhVTL 同款做法；虚拟目标命令永不挂起，返回 SUCCESS 阻止中层对虚拟硬件升级 device/host reset，本身正当，与 Kylin 归因无关。

**裁定**：保留。

## 七、K6（保留）：install.sh st warmup

`install.sh:711-722`：insmod 后对每个 st 节点 `mt status` + 双重 `mt rewind`。用户态、防御性、失败无后果（`|| true`）。

**裁定**：保留。

## 八、K7（复核通过）：MOVE MEDIUM sense 映射

`c1247e8` 的 errno→sense 映射复核：

- `-ENODEV`（源空）→ 3B/0D、` -EBUSY`（目标满）→ 3B/0E、`-EINVAL`（地址越界）→ 21/01、其他 → 04/00，分支覆盖 `vtl_changer_move_medium` 的全部返回路径（已逐个核对 `vtl_tape.c:1504-1768` 的 -EINVAL/-ENODEV/-EBUSY 产生点，无一遗漏）；
- 源空预检（storage/drive/IE 三分支）与 `vtl_changer_move_medium` 内部的 -ENODEV 是双保险，预检命中后 -ENODEV 只剩竞态窗口，两路同码 3B/0D，一致；
- 用户态 `vtl_misc.c:263`（VTL_IOCTL_MOVE）直接透传 errno 给 vtladm/对账脚本，不受 sense 映射影响；
- 21/01 是 SMC-3 对 changer 定义的 INVALID ELEMENT ADDRESS 正确编码（21/00 是 SBC 的 "unaligned write / LBA out of range"，sg3_utils 此前打印的 "Logical block address out of range" 正来源于此）。

**验证清单**（目标机重编 vtl.ko 后）：

```bash
# 目标槽满：往已占用槽位搬 → 期望 3B/0E
# 源槽空：从空槽搬出 → 期望 3B/0D
# 地址越界（如 dest=0x0300）→ 期望 21/01，sg_raw 打印 "Invalid element address"
# 正确 CDB（storage 3→6）：a5 00 00 00 00 03 00 06 00 00 00 00
```

## 九、同类误判归因残留扫描

全库扫描 `offline|Kylin` 后分类：

- **已更正**：`vtl_scsi.c`（TUR、post-move、MOVE MEDIUM 预检）、`vtl_main.c`（EH、offline guard）、`CLAUDE.md`、`install.sh` —— `613f391` + `e82d1cd`。
- **待改（随 K1 一并处理）**：`vtl_scsi.c:2327` queuecommand 注释 "Auto-restore VTL devices that Kylin st driver offlines"。
- **不属本次误判、保留**：SILI=0 零填充（`vtl_scsi.c:1121-1127`，实测确认的真 bug，与宕机无关）；scan/st_probe 竞态一族（`vtl_main.c` 串行 bringup/stagger 参数族，独立现象、独立修复）；rmmod-with-open kdump（`userspace/src/scsi_tape_holders.rs:2` 等，正是本次确认的真实根因方向）；`__offline__` 库（用户态业务概念，与设备离线无关）；历史报告 `code-review-report.md:15,139`（档案不改）。

## 十、处置建议汇总（按优先级）

1. **K1**：删除 queuecommand 入口的设备恢复段（含其旧归因注释），随下个内核改动窗口提交。
2. **K2**：改为 `mod_delayed_work(offline_guard, 0)` 或保留，二选一，建议前者。
3. **K3–K6**：维持现状。
4. 回归验证：裁剪 K1/K2 后，在 openEuler 6.6 目标机跑 `vtl-kernel-stability.sh phase-a-idle` + 备份软件走一轮 MOVE MEDIUM 密集操作，确认无设备离线残留。

—— 以上，本报告仅审查，未改动代码。
