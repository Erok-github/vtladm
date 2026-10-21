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

## 十、本轮审查的诚实边界

- EIO 归因（第九节主假设）已能解释全部观测，但"sidecar used > 文件大小"尚未经第九节命令对证实；sg_raw sense 数据可最终定音。
- W1/R1/W2/W3 为静态确认的代码缺陷；W4/W5 的修复方案以待现场确认后定稿。
