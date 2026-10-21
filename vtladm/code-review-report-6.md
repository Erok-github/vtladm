# vtladm 虚拟磁带库（VTL）项目代码审查报告（第六轮 · 1G 磁带写入 831MB 报错 + BOT 读 EIO）

- 审查日期：2026-10-21（第六轮，接续 `code-review-report.md`、`-2.md`、`-3.md`、`-4.md`、`-5.md`）
- 审查范围：针对现场故障——1G 容量磁带 tar 写入 831MB 报错中止，随后 `tar -tvf /dev/nst0` 在 BOT 读即 EIO，`mt erase/rewind/erase` 全部"成功"后再读仍 EIO——对磁带数据路径（`vtl_tape.c` 读写/冲刷、`vtl_scsi.c` READ/WRITE/ERASE 分发与 sense 映射）做全链路静态审查。
- 审查方式：纯静态审查。本机 Windows，目标 Linux，未编译未运行。
- 代码基线：分支 `vtladm26`，提交 `f017d5a`。
- 结论先行：**3 个确认缺陷（W1 高危、R1 高危、W2 中危）+ 1 个次要观察（W3）。写入中止的根因最可能是磁盘满且被 W1 掩盖；erase 后 BOT 读仍 EIO 无法由当前代码路径自洽解释，唯一剩余解释是磁带镜像文件的底层存储读错误，需现场数据定音（见第七节诊断清单）。**

---

## 一、故障现象复述

1. 用户经 vtladm 创建若干 1G 容量磁带，tar 向 /dev/nst0 写入 831MB 报错中止。
2. `tar -tvf /dev/nst0`：`Cannot read: Input/output error`（errno = EIO，硬错误）。
3. `mt erase` / `mt rewind` / `mt erase` 均返回成功。
4. 再次 `tar -tvf /dev/nst0` 仍 EIO。
5. `mtx status`：DTE0=VTLF938EA4A、DTE1=VTL8A44DE42，机械手面正常。

关键事实：erase 在内核侧将 `meta.used` 清零（`vtl_scsi.c:1304`），读路径在 `pos >= meta.used` 时只可能返回 EOD（BLANK CHECK/EOF），**不可能返回 -EIO**（详见第五节推理链）。erase"成功"后读仍 EIO，是本次审查的核心矛盾点。

---

## 二、缺陷总表

| # | 级别 | 位置 | 摘要 |
|---|------|------|------|
| W1 | **高危** | `vtl_tape.c:1130-1137, 1264-1270` vs `vtl_scsi.c:1167-1172` | 底层写失败 -ENOSPC（磁盘满）被折叠成 -EIO，SCSI 层准备好的 VOLUME_OVERFLOW 映射永远接不到 -ENOSPC |
| R1 | **高危** | `vtl_tape.c:1492-1493`（唯一复位点）vs `779-828/830-876`、`vtl_scsi.c:1278-1327/1188-1232` | 读游标 `rec_read_idx/rec_read_total` 只在 rewind 复位，装载/卸载/erase/LOCATE/SPACE 均不复位，跨磁带残留 |
| W2 | 中危 | `vtl_tape.c:1279, 1301` | 容量钳制截短写入时 `*actual = len`（应为 `to_write`），向上层虚报已写字节数 |
| W3 | 低 | `vtl_scsi.c:1302` | erase 在清零边界前先 flush 暂存记录：满盘时必然失败且返回被忽略，纯属浪费 I/O |

---

## 三、W1（高危）：磁盘满被误报为介质错误 —— 写入中止的最可能根因

### 3.1 代码事实

`vtl_rec_flush` 数据区写入失败分支（`vtl_tape.c:1130-1137`）：

```c
ret = kernel_write(tape->file, drv->rec_buf, saved_used, &pos);
if (ret < 0) {
    if (!retain) { drv->rec_buf_used = 0; drv->rec_count = 0; }
    return -EIO;          /* <-- 这里：kernel_write 的 -ENOSPC 被吃掉 */
}
```

大记录单块直写分支（`vtl_tape.c:1264-1270`）：

```c
ret = kernel_write(tape->file, shdr, sizeof(shdr), &pos);
if (ret == (ssize_t)sizeof(shdr))
    ret = kernel_write(tape->file, buffer, to_write, &pos);
if (ret < 0)
    ret = -EIO;           /* <-- 同样吞掉 -ENOSPC */
```

而 SCSI 层的映射本来是正确的（`vtl_scsi.c:1167-1172`）：

```c
if (ret == -EROFS)       vtl_set_sense(..., DATA_PROTECT, 0x27, 0);
else if (ret == -ENOSPC) vtl_set_sense(..., VOLUME_OVERFLOW, 0x00, 0);
else                     vtl_set_sense(..., MEDIUM_ERROR, 0x03, 0);
```

### 3.2 推理链

- 用户建了"一些"1G 磁带。**磁带镜像是按容量生成的文件，写多少占多少实际块**（即便稀疏创建，写入 831MB 真实数据就消耗 831MB 磁盘空间）。多块 1G 磁带 + 原有数据，`/opt/vtladm/var/tapes` 所在文件系统在写入中途打满是完全可能的场景。
- `kernel_write` 在底层文件系统满时返回 **-ENOSPC**。经 W1 折叠后，tar 收到的是 `MEDIUM_ERROR 0x03/0x00`（UNRECOVERABLE READ/WRITE ERROR）——表现为"I/O error"，与现场"没写完就报错"吻合。
- 若正确映射为 `VOLUME_OVERFLOW`（SSC 语义：到介质尾），备份软件/tar 会明确报"无空间/需换带"，用户第一次就能定位到磁盘容量问题，而不是怀疑 VTL 代码。
- 831MB 与 1G（10⁹ 或 2³⁰）之间不存在任何 VLBK 记账边界（块头开销约 0.01%），**容量检查本身（`vtl_tape.c:1220-1228`）不会在中途触发**，排除"磁带逻辑容量到顶"。

### 3.3 修复建议

- `vtl_rec_flush`：`kernel_write` 返回 <0 时原样透传 errno（`-ENOSPC` 保持 `-ENOSPC`，真 I/O 错误才 `-EIO`）；short write 分支已有正确的 -ENOSPC 语义，无需动。
- `vtl_tape_write` 单块分支：`ret < 0` 时区分 `ret == -ENOSPC`。
- SCSI 层 `vtl_handle_write` 已有正确映射，零改动。

---

## 四、R1（高危）：读游标跨磁带/跨操作残留，装载后不 rewind 的磁带"读出来是空的"

### 4.1 代码事实

`rec_read_idx/rec_read_total` 是 `struct vtl_drive` 上的多记录块（VLBK）读游标（`vtl.h:252-253`）。全仓库**唯一复位点**是 rewind（`vtl_tape.c:1492-1493`）。以下操作直接搬动 `tape->position` 或更换载带，但均不复位游标：

| 操作 | 位置 | 是否复位游标 |
|------|------|:---|
| REWIND | `vtl_tape.c:1471-1502` | ✅ 唯一复位 |
| LOAD（换带） | `vtl_tape.c:779-828` | ❌ |
| UNLOAD | `vtl_tape.c:830-876` | ❌ |
| ERASE | `vtl_scsi.c:1278-1327` | ❌ |
| LOCATE | `vtl_scsi.c:1188-1232` | ❌ |
| SPACE | `vtl_tape.c:1314+` | ❌ |

### 4.2 故障机理

读路径入口（`vtl_tape.c:921`）：`if (drv->rec_read_idx >= drv->rec_read_total)` 才会读取新 VLBK 块头。若上次在磁带 A 上读到一个多记录块的中间（idx < total），随后换带/erase/locate 到磁带 B 的 BOT：

1. 首读跳过块头解析（判据为假）；
2. 直接按"块内第 idx 条记录"从 B 的 position 处解析 `rec_sz`——读到的是 B 带首块的原始字节，通常解析出天文数字；
3. `rpos + rec_sz > meta.used` 越界检查（`:952`）触发 → 返回 **EOD**；
4. 且该分支**不推进游标** → 此后每次读都重复 EOD，直到有应用显式 rewind。

后果：任何在驱动器上"读过一半 → 换带/erase/locate → 直接读"的序列，该磁带表现为**整盘空白**，`tar -tvf` 报"This does not look like a tar archive"类错误。备份软件轮询介质内容时会得到错误结论。这正是 mt/mtx 之外最常见的工作流（机械手自动换带后 st 直接续读）。

### 4.3 与本次现场的关系

用户执行过 rewind（游标已清），故 R1 **不直接解释**本次的 EIO；但它是同源代码区的独立高危缺陷，且掩盖了"erase 后读不出"类问题的定位（游标残留时现象同样是"erase 后像空带"），必须一并修。

### 4.4 修复建议

新增 `vtl_read_cursor_reset(drv)`（两行赋值），在 LOAD / UNLOAD / ERASE / LOCATE / SPACE / 写路径 BOT 重置（`vtl_tape.c:1212-1216`）处统一调用——即**凡在非读路径改动 `tape->position` 或更换 `loaded_tape` 之处都复位游标**，把不变量收敛为"游标只在读块头后、块内消费期间有效"。

---

## 五、关于"erase 后 BOT 读仍 EIO"的归因分析（未闭环，需现场数据）

### 5.1 静态推理链

erase 成功后：`tape->position=0`、`meta.used=0`（`vtl_scsi.c:1303-1304`），sidecar 同步写 0（`:1305-1306`），filemark 清空并持久化（`:1310-1321`）。

此时 `vtl_tape_read` 所有可能的返回：

- `pos(0) >= used(0)` → 行 `:910` 返回 0 → `vtl_handle_read` 发 **NO SENSE 0x00/0x05**（EOD）→ st 转为 EOF（该路径此前 openEuler 6.6 实测通过）；
- 向下绝无可能进入任何 `-EIO` 分支——所有 `-EIO` 来源（行 `:931/947/964/1011/1040/1058/1068`）都要求 `pos < used` 且实际执行 `kernel_read`。

**结论：只要 erase 真的执行到了，erase 后的读在 VTL 代码内不可能产生 EIO。** 现场却观测到 EIO，二者矛盾，只剩以下自洽解释：

1. **磁带镜像文件本身的底层读 I/O 错误**（最可能）：磁盘介质/文件系统故障时，`kernel_read` 返回负 EIO。但这与 `pos >= used` 短路冲突——除非 **sidecar 里的 `used` 不是 0**（见 2）。
2. **erase 的 `vtl_meta_write` 失败导致 sidecar 残留旧 used**：`vtl_meta_write` 的返回值在 erase 里**没有被检查**（`vtl_scsi.c:1305-1306`）。若文件系统已满（与 W1 根因同源！），sidecar 写失败 → 磁盘上 used 仍是 831MB。但当前运行会话内存中 used=0，读仍走 EOD……**除非磁带对象被重新打开过**——`vtl-robot-align.timer`/`vtl-robot-sync.timer` 的周期对账、或 web 管理面对磁带的任何 reload 操作，都会经 `vtl_meta_read` 从 sidecar 恢复 `used=831MB`；而镜像文件真实大小可能只有部分落盘（满盘时短写）。此后 BOT 读按 831MB 走，越界/短读分支触发 -EIO（行 `:931/947/964`）——**与 EIO 现象自洽**。
3. st 侧状态异常（设备被 offline 等）：mt 命令成功执行可基本排除。

第 2 条链条把 W1（满盘）与 EIO 连成同一根因：**磁盘满 → 写失败（W1 掩盖为介质错误）→ erase 的 sidecar 写静默失败 → 对账/重开磁带后 used 与真实文件撕裂 → 读 EIO**。这是当前唯一能同时解释全部观测的假设。

### 5.2 需要用户提供的现场数据（按优先级）

1. `dmesg -T | tail -100` 与 `dmesg | grep -iE "I/O error|critical|EXT4|XFS|buffer_io_error"` —— 底层块层/文件系统错误，以及 VTL 自身 pr_warn（`rec block header fixup failed`、`ERASE failed to persist`）。
2. `df -h /opt/vtladm/var/tapes` 与 `df -i` —— 磁盘满验证。
3. `ls -la /opt/vtladm/var/tapes/` —— 各 `.vtltape` 镜像与 sidecar 的大小对比（sidecar 内 used 与文件实际大小是否撕裂）。
4. 复现时刻的 sense 数据：`sg_raw -v -r 32768 /dev/sg7 08 00 00 00 00 00`（READ(6)）——返回的 sense key/ASC/ASCQ 可一锤定音区分 EOD（0/0/05）、MEDIUM_ERROR（3/xx）、NOT_READY（3A/xx）。
5. 写报错时 tar 的完整输出文本。

---

## 六、W2（中危）：容量钳制截短时向上层虚报写入长度

`vtl_tape_write` 在接近逻辑容量时把写入截短：`to_write = min(len, capacity - position)`（`:1227-1228`），但两个成功分支都报告 `*actual = len`（`:1279`、`:1301`）。当 `to_write < len` 时，发起方（st/备份软件）认为 len 字节全部写入，实际落盘的记录只有 to_write 字节——造成**静默数据截断**，读回时记录长度与预期不符（表现为"磁带被截断"类错误）。应改为 `*actual = to_write`（并确保 `to_write==0` 的边界走 -ENOSPC）。

## 七、W3（低危）：erase 前多余的 flush

`vtl_scsi.c:1302`：erase 在 `used=0` 之前先 `vtl_rec_flush(drv, tape, false)` 把暂存记录写盘——erase 的语义是丢弃全部数据，先把缓冲区落盘再清零边界纯属浪费；满盘场景下必然失败（返回被忽略）。建议改为直接丢弃缓冲区（`rec_buf_used = 0; rec_count = 0;`）。

---

## 八、修复建议汇总（待用户确认后实施）

| # | 修复点 | 预计改动 |
|---|--------|---------|
| W1 | `vtl_rec_flush:1130-1137`、`vtl_tape_write:1264-1270` 透传 -ENOSPC | ~6 行 |
| R1 | 新增游标复位 helper，LOAD/UNLOAD/ERASE/LOCATE/SPACE/写 BOT 重置处调用 | ~10 行 |
| W2 | `*actual = to_write` | 2 行 |
| W3 | erase 前丢弃 rec_buf 而非 flush | 2 行 |
| 附带 | erase 检查 `vtl_meta_write` 返回值，失败时 pr_warn（本次 EIO 排查的关键日志） | 2 行 |

同时建议：在拿到第五节诊断数据前，**不要在故障机上继续写入**，以免满盘状态下的短写进一步撕裂镜像与 sidecar。

---

## 九、补充现场证据分析（2026-10-21 tar -tvf 完整输出）

用户在第五节后补上了 `tar -tvf /dev/nst0` 的完整输出，三条新事实显著收窄了归因范围：

1. **磁带前段 ~800MB 数据完好可读**：tar 连续列出了大量文件（gcc ChangeLog 系列、sg3_utils、systemd 文档等）。写路径主体功能正常，之前 10-13 的读写测试结论不受影响。
2. **几乎每隔几个文件就插入一次 "Skipping to next header"**：tar 字节流中周期性出现非 tar 结构区段。两个候选机制：
   - **读路径零填充**（`vtl_tape.c:967-968、1019-1020、1124-1127`）：每条短记录被零填充到 SCSI 缓冲区全长并上报等量传输。若 st 的 READ 缓冲区大于 tar 记录（tar 默认 20×512=10240 字节），每条记录后都尾随一段 zeros，tar 在期望下一头标的位置撞到零块 → skip 后重新同步——与观测到的"隔几个文件一次 skip"分布吻合；
   - 写阶段磁盘满造成的记录级撕裂散布在中途（VLBK 短写修复逻辑本身是记录对齐的，单纯冲刷撕裂不会造成散布式错位，故此候选优先级较低）。
3. **末尾在同一位置连续 11 次 EIO 后放弃**：read 路径在同一偏移反复返回 -EIO，位置不前进（`-EIO` 分支均不回写 `tape->position`），tar 每次重试原位移——与 **"meta.used（sidecar）> 镜像文件实际大小"** 的撕裂假设完全自洽：读指针越过真实 EOF 后，`kernel_read` 返回 0 ≠ 期望字节数（`vtl_tape.c:931/947`）→ -EIO。

**更新后的主假设**：磁盘满 → 写中止（W1 掩盖为介质错误）→ 满盘期间 sidecar/镜像持续撕裂（`vtl_meta_write` 返回值多处未检查）→ 读时在 used 边界撞上真实 EOF → 固定位置重复 EIO。零填充问题（假设 2a）独立存在，决定 tar 能否平滑读完未撕裂区段。

### 撕裂判定的一对命令（无需重启、只读）

sidecar 布局（`vtl_tape.c:109-116`，20 字节 packed）：偏移 0-3 magic "VLTM"、4-5 version、6 density、7 flags、**8-15 used（u64 LE）**。

```bash
# 1. 内核认为的已用字节数（sidecar）
for f in /opt/vtladm/var/tapes/*.vtlmeta; do
  printf '%s  used=%s\n' "$f" \
    "$(xxd -p -l 8 -s 8 "$f" | sed 's/\(..\)\(..\)\(..\)\(..\)\(..\)\(..\)\(..\)\(..\)/\8\7\6\5\4\3\2\1/' | sed 's/^0*//')"
done

# 2. 镜像文件的真实大小
ls -la /opt/vtladm/var/tapes/*.vtltape

# 3. 对比：used 与 stat size。flush 每次把 used 推到已写字节末尾，
#    正常情况下 used(+) 头开销 ≈ 文件实际大小。used 明显大于文件大小 = 撕裂实锤。
```

另请补：`dmesg -T | grep -iE "VTL|I/O error|EXT4|XFS" | tail -30`、`df -h /opt/vtladm/var/tapes`、以及出错位置上 `sg_raw -v -r 32768 /dev/sg7 08 00 00 00 00 00` 的 sense 数据。

### 对修复清单的影响

- W1/R1/W2/W3 结论不变，均应按第八节修复。
- **新增 W4（中危，待现场确认后定级）**：读路径将短记录零填充到 SCSI 缓冲区全长并虚报传输长度（三处）。若 sg_raw 实测证实，应改为按真实记录长度传输并设置 `scsi_set_resid`，由 initiator 按 SSC 变量块语义处理短块（SILI/residual），而不是替 st 做主。
- **新增 W5（中危）**：`vtl_meta_write` 返回值在 erase（`vtl_scsi.c:1305-1306`）、flush（`vtl_tape.c:1170-1171、1275-1276`）等路径全部未检查——sidecar 静默过期正是"used 与真实文件撕裂"且无任何日志的直接原因。至少应 pr_warn。

## 十、现场数据定音（2026-10-21 dmesg / df / ls / sg_raw 全套记录）

用户提供 `dmesg -T | tail -100`、`df -h`、`ls -la /opt/vtladm/var/tapes/`、`sg_raw READ(6)` 原始记录后，本次事故的根因可以**闭合**，且与第五节假设（磁盘满/撕裂）不同——**磁盘没有满，磁带数据没有撕裂，VTL 读写路径全程健康**。真正的元凶是用户空间同步守护进程与操作员/备份作业的竞态。

### 10.1 记录中的硬事实

1. **`df -h /opt/vtladm/var/tapes`：69G 总量、20G 可用（70%）——磁盘从未满**。W1（ENOSPC 误映射）不是本次事故触发点（仍是应修的潜伏缺陷）。
2. **全部 5 块磁带镜像均为精确的 1073741824 字节（1GiB 预分配），且 .vtltape/.vtlmeta 硬链接数=2**（`mars/` 子目录与 `__offline__/` 各持一份引用）——按容量预分配的文件，"文件大小"不反映 used，第九节"used vs 文件大小"比对法对本部署不适用。
3. **内核留有逐命令日志**（`vtl_scsi.c:2380-2383`，非 GOOD 即 pr_info）：09:32:18 ERASE → CC；**紧随其后的 st sense 打印：`Sense Key: Not Ready / Medium not present`**；09:32:19 WRITE → CC（同样 medium not present）；09:36:16/17 重复一次。——**用户两次 `mt erase` 全部失败（驱动器内无带），"erase 成功"是错觉**；mt 未把错误呈现给用户（或未被注意）。
4. **09:32 之前驱动器 0 最后一次装载是 09:21:37，此后到 09:32:18 之间 dmesg 没有任何 `Moved medium` 记录**——驱动器从"有带"变成"空"没有经过 MOVE MEDIUM 路径。
5. 09:44:32 **连续 11 条 READ CC**（与 tar 输出 "Cannot read" ×11 逐条对应，位置不前进）；09:45:01、09:53:12 各一条 READ CC。
6. 10:23 sg_raw READ(6)：sense = **NO SENSE 0x00/0x05（End-of-data）**，32KB 全零——磁带在 10:23 时已在驱动器内、位置=数据尾（831MB 档案仍在带上，erase 从未执行成功）。
7. dmesg 全段无一条 I/O error / fs error / `rec block header fixup failed` / `Short write on meta file`——内核写路径零报错。
8. `vtl-robot-sync.timer`：**每 5 分钟**一次（`OnBootSec=2min, OnUnitActiveSec=5min`）；守护进程（`userspace/src/robot_sync.rs`）经 `/dev/vtl` ioctl 直接下发 `VTL_IOCTL_LOAD_TAPE / UNLOAD_TAPE / SLOT_PLACE / MOVE_MEDIUM`（`robot_sync.rs:313/336/354/531`）。**ioctl 卸载不经过 MOVE MEDIUM，不打 "Moved medium" 日志**——这正是事实 4 的解释。

### 10.2 事故时间线（与全部观测自洽）

| 时刻 | 事件 |
|------|------|
| 09:21:37 | 磁带装入驱动器 0（机械手面操作，伴随 .vtlfm 告警）；随后 tar 开始写档案 |
| 09:21–09:32 | 写入 ~831MB，**全部 GOOD**（dmesg 无任何 WRITE CC）——数据完好落盘 |
| ~09:32 | **robot-sync 5 分钟周期到点**：DB 认为该带应在槽位，下发 UNLOAD_TAPE ioctl，**把正在写入的磁带从驱动器里抽走（无日志）** |
| 09:32:19 | tar 的下一条 WRITE 收到 `Medium not present` → **"写入 831MB 报错"** |
| 09:32:18 / 09:36:16 | 用户两次 `mt erase`：驱动器已空 → CC（mt 未呈现错误）→ 用户误以为 erase 成功 |
| ~09:41–09:44 前 | 下一轮 sync 按 DB 映射把磁带装回驱动器 0 |
| 09:44:32 | tar -tvf 正在读档列举文件（故用户看到了大量文件清单）；**sync 再次到点抽带** → 随后 11 条 READ 全部 `Medium not present` → tar 报 EIO ×11 放弃。此前间歇出现的 "Skipping to next header" 是 tar 从非 BOT 位置/中断点重新同步的正常行为 |
| 09:45 / 09:53 | 用户对空驱动器的两次读尝试 → CC |
| 10:23 | 磁带已被装回，位置在数据尾 → sg_raw 读到 EOD sense、全零数据 |

### 10.3 根因裁定

**R2（本次事故根因，高危）：`vtl-robot-sync.timer`（5 分钟周期）在驱动器被占用时直接卸载/装载磁带。**

- 同步守护进程的语义本是"kernel → DB 盘点回填"，但它同时做反向纠错（DB → kernel 强制执行槽位/驱动器映射，`robot_sync.rs:336/354`），且**没有任何"驱动器是否被占用"的检查**；
- 卸载走私有 ioctl，绕过 MOVE MEDIUM 路径，**内核与用户都无日志可查**（`Moved medium` 与 web 审计均不覆盖），排障时完全隐形；
- 5 分钟周期对"纠错"行为而言过于激进；操作员手工操作/备份作业的时间窗必然与其碰撞；
- 与 10-13 的"web 仪表盘数据和库管理对不上"同源：DB 与内核双写真相，互为纠错源。

**修复方向（按优先级）**：
1. sync 守护进程下发 LOAD/UNLOAD 前必须检查驱动器占用：用户态可查 `/proc/*/fd` 指向 `/dev/nst*`、或经 ioctl 查询驱动器最近 I/O 时间，近期有活动则跳过本轮纠错（下次再试）；
2. 内核 `VTL_IOCTL_UNLOAD_TAPE` 路径增加同等的"近期有 I/O 则 -EBUSY"闸门（与 `vtl_preflight_before_rmmod()` 同一思路）；
3. 反向纠错（DB→kernel）默认改为只告警不执行；确需强制执行时走带审计日志的通道；
4. 备份窗口内由部署脚本停掉 sync/align 定时器（install.sh 已具备停 timer 的能力，补一条文档/运维建议即可）。

### 10.4 其他发现（独立于事故）

- **S1（低）**：`vtl_scsi.c:2381` 每条非 GOOD 磁带命令 pr_info——错误风暴期刷 dmesg 且拖慢 I/O；应降为 `pr_debug` 或限流。
- **S2（低）**：mars_Tape01 的 `.vtlfm` 含一条 offset=0 的文件标（`idx 0 off 0`），触发 `vtl_tape.c:646-652` 整组拒绝 + 每次装载告警。建议：校验时跳过/容忍单条 0 偏移（或落盘时过滤），且同一张带重复装载时告警限流。
- **S3（观察）**：磁带镜像与 .vtlmeta 均为硬链接（link=2），`mars/` 与顶层目录各持一份——需确认 web 面/离线目录操作不会经第二条路径改写同一 inode（内容共享，O_TRUNC 会互相影响）。
- **W4 降级为"未证实"**：skip 模式可由 10.2 的非 BOT 起点解释，不再作为疑似根因；是否保留零填充行为待 sg_raw 在数据区段直接验证（BOT 处 `sg_raw -v -r 32768 /dev/sg7 08 00 00 00 00 00`，若 10KB 记录返回 32KB 全填充则另议）。
- **W1/W2/W3/R1/W5 维持原判**：静态确认的潜伏缺陷，与本次事故无关，仍应修复。

### 10.5 用户侧即时处置

1. 手工测试/备份窗口内停掉同步：`systemctl stop vtl-robot-sync.timer vtl-robot-align.timer`（测试完再 start）；
2. 磁带数据未损坏：`mt rewind && tar -tvf /dev/st0`（注意用**回绕设备 /dev/st0** 而非 nst0）应能完整列出 831MB 档案；
3. 以后 `mt erase` 后务必用 `mtx -f /dev/sg6 status` 或 dmesg 验证，不要以 mt 的静默返回为准。

## 十一、最终定音（2026-10-21 sg_raw sense 数据）：EOD sense key 写错

### 11.1 一锤定音的证据

`sg_raw -v -r 32768 /dev/sg7 08 00 00 00 00 00`（磁带在驱动器、BOT 位置）返回：

```
SCSI Status: Check Condition
Sense key: No Sense
Additional sense: End-of-data detected   (0x00 / 0x00 / 0x05)
```

对照 `vtl_scsi.c:1111-1115`：

```c
/* End of data: SSC tape targets signal EOD via CHECK CONDITION
 * with BLANK CHECK (0x00/0x00/0x05), not via GOOD+residual.
 * Both Kylin 4.19 SILI=0 and openEuler 6.6 st drivers recognise
 * this as end-of-recorded-data and stop reading. */
vtl_set_sense(&drv->sense, 0, 0x00, 0x05);   /* ← key=0 (NO SENSE)，注释写的是 BLANK CHECK */
```

**注释声明的行为（BLANK CHECK）与代码实现（NO SENSE）不一致**——这是本案最后一个、也是最隐蔽的一个 bug：openEuler 6.6 的 st 对 `NO SENSE 0/0/05` 不识别为 EOD，向上层返回 **-EIO**（现场实测：tar 报 "Cannot read: Input/output error"）。

**源码级核实（2026-10-21，拉取 torvalds/linux v4.19 与 v6.6 的 drivers/scsi/st.c 逐行对比）**：

1. 两个版本 `read_tape()` 的 sense 处理**完全相同**：`st_analyze_sense()` 从 sense byte2 取 FM/EOM/ILI 标志（`flags = sense[2] & 0xe0`），残余量只认 sense 信息字段（VALID 位 + bytes 3-6）；随后 `if (flags != 0)` 走 ILI/FMK/EOM 分支，**`else`（flags==0）一律 `retval = -EIO`**，唯一的例外是"BLANK_CHECK 且恰在文件标之后"（置 `ST_EOD_2`）。
2. 因此 **`NO SENSE 0/0/05` 在 4.19 上同样返回 EIO**——代码注释中"Kylin 4.19 st 可识别"的说法**从来不成立**（当年 Kylin 测试应未读到 EOD，例如只读了已知字节数即停止）。st 是上游通用驱动，发行版无补丁，差异只可能来自内核版本，而此处 4.19/6.6 无差异。
3. st 唯一能翻译成干净 EOF（read()=0）的磁带结束信号是：**sense byte2 的 EOM 标志位 + VALID 信息字段（残余量）** → 走 `SENSE_EOM` 分支，`buffer_bytes = bytes - uremainder = 0`。纯 `BLANK_CHECK/0/0/05`（无 EOM 位）在普通位置首次读取同样落 `-EIO` 分支。
4. 真实磁带的行为（HP LTO Ultrium 5 技术手册卷 2）：READ 遇 EOD 返回 **sense key 8h（BLANK CHECK）、ASC/ASCQ 00/05**；EOT 类条件明确"**The EOM flag will be set**"并附残余量；st 源码中 `flags &= 0xcf; /* No need for EOM in this case */`（BLANK_CHECK 时清除 EOM）也证明 st 作者预期 BLANK_CHECK 常与 EOM 位同现。三者互证：**真实 LTO 在 EOD 时返回 BLANK_CHECK + EOM 位 + 有效残余量**。

### 11.1a F1 修复配方（修正初版"仅改 key"的不足）

`vtl_build_sense_buffer()` 目前只写 0x70/key/ASC/ASCQ，不带标志位与信息字段。EOD 分支应构造与真实 LTO 等价的 sense：

| 字节 | 现值 | 应改为 | 含义 |
|------|------|--------|------|
| byte 0 | 0x70 | **0xF0** | VALID 位置 1（信息字段有效） |
| byte 2 | key=0x00 | **0x48** | key=BLANK_CHECK(0x08) + **EOM 位(0x40)** |
| bytes 3–6 | 0 | **剩余未传输字节数（BE）** | 一次 READ 请求长度（0 字节已传输） |
| byte 12/13 | 0/5 | 0/5 | ASC/ASCQ 不变 |

预期 st 行为（4.19/6.6 一致）：read() 返回 0（EOF），`eof=EOM_OK`；后续 read 重复同 sense、继续返回 0，tar/备份软件正常收尾。修复同时需更正 1113 行错误注释。

### 11.2 事故全貌（全部观测闭合）

| 时刻 | 观测 | 归因 |
|------|------|------|
| 09:21–09:32 | tar 写 831MB，内核零报错 | 写入本身完全正常 |
| 09:32:19 | WRITE 收到 `Medium not present` | **R2**：robot-sync（5min 周期）ioctl 抽带，写到一半介质消失 |
| 09:32:18 / 09:36:16 | 两次 ERASE 均 CC（medium not present），mt 未呈现错误 | 驱动器已空，erase 从未执行；"erase 成功"是错觉 |
| 09:44:32 | tar -tvf 列出全部 ~831MB 文件后，11 条 READ CC → EIO×11 | **F1**：读完数据撞 EOD，st 把 0/0/05 误译为 EIO。磁带内容完好，"Skipping to next header"为 tar 从中断位置重新同步的正常行为 |
| 09:45/09:53 | 单条 READ CC | 对空驱动器/空带的重复尝试，同 F1 |
| 10:23 / 10:40 | TUR GOOD；READ=0/0/05 EOD | 当前 DTE0 装载的是**空带（used=0）**——sync 来回装卸把磁带洗牌了；档案磁带在槽位里。空带 BOT 即 EOD 属正确行为，但 st 仍报 EIO（F1） |

**用户数据完好**：831MB 档案在磁带上原封未动，erase 从未成功；读出全部内容的唯一障碍是 F1 的 sense key。

### 11.3 修复清单（定稿）

| # | 级别 | 位置 | 修复 |
|---|------|------|------|
| **F1** | **高（本案主修复）** | `vtl_scsi.c:1115`、`vtl_build_sense_buffer()` | EOD sense 按 §11.1a 配方构造：BLANK_CHECK(0x08) + EOM 位 + VALID + 信息字段=未传输字节数；更正 1113 行错误注释 |
| **R2** | **高（写中断根因）** | `userspace/src/robot_sync.rs`、`/dev/vtl` UNLOAD/LOAD 路径 | 卸载/装载前检查驱动器占用（用户态查 `/proc/*/fd`→`/dev/nst*`，内核态 ioctl 加"近期有 I/O 则 -EBUSY"闸门）；反向纠错默认降级为告警 |
| W1 | 中 | `vtl_tape.c:1130-1137,1264-1270` | 透传 `kernel_write` 的 -ENOSPC，不再折叠为 -EIO |
| R1 | 高 | `vtl_tape.c` load/unload/erase/locate/space | 读游标 `rec_read_idx/total` 在非读路径位置变更处统一复位 |
| W2 | 中 | `vtl_tape.c:1279,1301` | 容量钳制时 `*actual = to_write` |
| W3 | 低 | `vtl_scsi.c:1302` | erase 前丢弃 rec_buf 而非 flush |
| W5 | 中 | `vtl_meta_write` 各调用点 | 检查返回值并 pr_warn |
| S1 | 低 | `vtl_scsi.c:2380-2383` | 逐命令 pr_info 降为 pr_debug 或限流 |
| S2 | 低 | `vtl_tape.c:646-652` | .vtlfm 单条 0 偏移容忍/过滤，装载告警限流 |

### 11.4 用户侧即时处置

1. 找到档案磁带：对比各 `.vtlmeta` 的 used（第九节 xxd 命令），used≈831MB 的就是；用 mtx 装入驱动器 0 后 `mt rewind && tar -tvf /dev/st0`（修好 F1 前，读完 EOF 处仍会报一次 EIO，属预期，可忽略或 `tar -tvf ... 2>/dev/null`）；
2. 测试/备份窗口内保持 `vtl-robot-sync.timer`、`vtl-robot-align.timer` 停止；
3. F1 修复重装后，同样场景 st 会正确返回 EOF，tar 行为恢复正常。

## 十二、本轮审查的诚实边界

- F1 有现场 sg_raw sense + st 行为实证，定音；R2 有 dmesg medium-not-present 时间线实证。
- W4（零填充）撤销：skip 模式由 11.2 的非 BOT 起点完整解释，无独立证据支持，维持现状观察。
- W1/W2/W3/R1/W5/S1/S2 为静态确认的独立缺陷，修复不影响 F1/R2 的验证。
