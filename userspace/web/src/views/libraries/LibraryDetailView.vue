<script setup lang="ts">
import { ref, onMounted, computed, h } from 'vue';
import { useRoute } from 'vue-router';
import {
  NCard, NGrid, NGi, NStatistic, NTag, NSpin, NProgress,
  NTabs, NTabPane, NButton, NDataTable, NInput, NSelect,
  NInputNumber, NSpace, NModal, NPopconfirm, NCheckbox,
  NEmpty, NSwitch, useMessage, NText,
} from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchLibraryDetail } from '@/api/libraries';
import { fetchShelves, createShelf, deleteShelf } from '@/api/shelves';
import {
  fetchTapes, deleteTape, initTape,
  createTapeAutoBatch, assignSlotBatch, shelfPlace,
  fetchDensityLimits,
} from '@/api/tapes';
import { fetchEmptySlots, fetchPatrol, fetchFabric } from '@/api/other';
import {
  fetchIscsiConfig, setIscsiAllowExec, fetchIscsiExportDefaults,
  libraryIscsiExport, libraryIscsiUnexport, scanTransportSg,
} from '@/api/other';
import { robotReconcile, robotAutoAlign, robotSync } from '@/api/changer';
import type {
  LibraryDetailResponse, TapeRow, ShelfRow,
  DensityLimit, DriftItem,
} from '@/api/types';

const route = useRoute();
const libraryStore = useLibraryStore();
const message = useMessage();

// ── common ──
const id = computed(() => route.params.id as string);
const libraryName = computed(() => {
  const lib = libraryStore.libraries.find((l) => String(l.id) === id.value);
  return lib?.name ?? '';
});
const detail = ref<LibraryDetailResponse | null>(null);
const loading = ref(true);

function fmtBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024 * 1024)).toFixed(1) + ' TB';
  if (b >= 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
  if (b >= 1024 * 1024) return (b / (1024 * 1024)).toFixed(0) + ' MB';
  return b + ' B';
}

// ── Tab 1: overview ──
const totalCap = computed(() => (detail.value?.tapes ?? []).reduce((s, t) => s + t.capacity_bytes, 0));
const totalUsed = computed(() => (detail.value?.tapes ?? []).reduce((s, t) => s + t.used_bytes, 0));
const usagePct = computed(() => totalCap.value > 0 ? Math.round((totalUsed.value / totalCap.value) * 100) : 0);
const emptySlotCount = computed(() => {
  if (!detail.value) return 0;
  return detail.value.changer.data_slots.filter((s) => !s.tape_name).length;
});

// ── Tab 2: shelves ──
const shelves = ref<ShelfRow[]>([]);
const shelvesLoading = ref(false);
const showCreateShelf = ref(false);
const newShelfName = ref('');

async function loadShelves() {
  if (!libraryName.value) return;
  shelvesLoading.value = true;
  try {
    const data = await fetchShelves(libraryName.value);
    shelves.value = data.shelves;
    refreshShelfOpts();
  } catch { console.error('加载货架失败'); } finally { shelvesLoading.value = false; }
}

function refreshShelfOpts() {
  tapeShelfOpts.value = shelves.value.map((s) => ({ label: s.name, value: s.name }));
}

async function handleCreateShelf() {
  if (!libraryName.value || !newShelfName.value.trim()) return;
  try {
    await createShelf(libraryName.value, newShelfName.value.trim());
    message.success(`货架 ${newShelfName.value} 创建成功`);
    showCreateShelf.value = false;
    newShelfName.value = '';
    await loadShelves();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '创建失败');
  }
}

async function handleDeleteShelf(name: string) {
  if (!libraryName.value) return;
  try {
    await deleteShelf(libraryName.value, name);
    message.success(`货架 ${name} 已删除`);
    await loadShelves();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '删除失败');
  }
}

// ── tape single-op (delete/init/shelf-place) ──
const showShelfPlaceModal = ref(false);
const shelfPlaceTarget = ref<TapeRow | null>(null);
const shelfPlaceTargetShelf = ref<string | null>(null);

async function handleTapeDelete(name: string) {
  if (!libraryName.value) return;
  try {
    await deleteTape(libraryName.value, name);
    message.success(`磁带 ${name} 已删除`);
    await reloadDetail();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '删除失败');
  }
}

async function handleTapeInit(name: string) {
  if (!libraryName.value) return;
  try {
    await initTape(libraryName.value, name);
    message.success(`磁带 ${name} 已清空`);
    await reloadDetail();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '清空失败');
  }
}

function openShelfPlace(row: TapeRow) {
  shelfPlaceTarget.value = row;
  shelfPlaceTargetShelf.value = row.shelf_name ?? null;
  showShelfPlaceModal.value = true;
}

async function handleShelfPlaceExec() {
  if (!libraryName.value || !shelfPlaceTarget.value) return;
  try {
    await shelfPlace(libraryName.value, shelfPlaceTarget.value.name, shelfPlaceTargetShelf.value ?? undefined);
    message.success(`磁带 ${shelfPlaceTarget.value.name} 已移至货架`);
    showShelfPlaceModal.value = false;
    await reloadDetail();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '换架失败');
  }
}

const shelfColumns: DataTableColumns<ShelfRow> = [
  { title: '名称', key: 'name', width: 160 },
  { title: 'ID', key: 'id', width: 60 },
  {
    title: '类型', key: 'type', width: 80,
    render: (r: ShelfRow) => r.is_default_unused ? '默认(未使用架)' : '自定义',
  },
  {
    title: '操作', key: 'actions', width: 80,
    render(row) {
      if (row.is_default_unused) return '——';
      return h(
        NPopconfirm,
        { onPositiveClick: () => handleDeleteShelf(row.name) },
        {
          trigger: () => h(NButton, { size: 'tiny', type: 'error' }, { default: () => '删除' }),
          default: () => `确定删除货架 ${row.name}？架上须无磁带`,
        },
      );
    },
  },
];

// ── Tab 3: batch tape create ──
const densityLimits = ref<DensityLimit[]>([]);
const selectedDensity = ref<number | null>(null);
const batchCount = ref(5);
const batchSize = ref('100G');
const batchShelf = ref<string | null>(null);
const tapeShelfOpts = ref<{ label: string; value: string }[]>([]);
const creating = ref(false);

const selectedDensityInfo = computed(() =>
  densityLimits.value.find((d) => d.code === selectedDensity.value) ?? null,
);

function parseSizeHuman(size: string): number {
  const m = size.trim().match(/^(\d+(?:\.\d+)?)\s*([KMGTP]?B?)$/i);
  if (!m) return -1;
  let n = parseFloat(m[1]);
  if (isNaN(n) || n <= 0) return -1;
  const unit = (m[2] || 'B').toUpperCase().replace(/B$/, '');
  const mult: Record<string, number> = { '': 1, K: 1024, M: 1024 ** 2, G: 1024 ** 3, T: 1024 ** 4, P: 1024 ** 5 };
  n *= mult[unit] ?? 1;
  return Math.round(n);
}

const capacityError = computed(() => {
  const bytes = parseSizeHuman(batchSize.value);
  if (bytes <= 0) return '请输入有效容量，如 100G、1T';
  const info = selectedDensityInfo.value;
  if (info && (bytes < info.min_bytes || bytes > info.max_bytes)) {
    return `超出 ${info.label} 容量范围: ${info.min_human} ~ ${info.max_human}`;
  }
  return null;
});

const namePrefix = computed(() => libraryName.value ? `${libraryName.value}_Tape` : 'Tape');

const namePreview = computed(() => {
  const pad = Math.max(2, String(batchCount.value).length);
  const end = batchCount.value;
  const fmt = (n: number) => String(n).padStart(pad, '0');
  if (end <= 5) return Array.from({ length: end }, (_, i) => `${namePrefix.value}${fmt(i + 1)}`).join(', ');
  return `${namePrefix.value}${fmt(1)} ~ ${namePrefix.value}${fmt(end)}`;
});

async function loadDensityLimits() {
  try {
    const data = await fetchDensityLimits();
    densityLimits.value = data.density_limits;
  } catch { console.error('API 调用失败'); }
}

async function handleBatchCreate() {
  if (!libraryName.value) return;
  if (capacityError.value) { message.warning(capacityError.value); return; }
  creating.value = true;
  try {
    const result = await createTapeAutoBatch(
      libraryName.value,
      batchCount.value,
      batchSize.value,
      batchShelf.value ?? undefined,
      selectedDensity.value != null ? String(selectedDensity.value) : undefined,
    );
    message.success(`批量创建 ${result.count} 盘磁带完成`);
    await reloadDetail();
    await loadTapesForAssign();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '批量创建失败');
  } finally { creating.value = false; }
}

// ── Tab 4: batch slot assign ──
const assignTapes = ref<TapeRow[]>([]);
const assignChecked = ref<Set<string>>(new Set());
const emptySlots = ref<number[]>([]);
const slotMappings = ref<Map<string, number>>(new Map());
const assignLoading = ref(false);

async function loadTapesForAssign() {
  if (!libraryName.value) return;
  try {
    const [t, s] = await Promise.all([
      fetchTapes(libraryName.value, 0, 5000),
      fetchEmptySlots(libraryName.value),
    ]);
    assignTapes.value = t.tapes.filter((tape) => !tape.in_drive && tape.shelf_name != null);
    emptySlots.value = s.empty_slots;
  } catch { console.error('API 调用失败'); }
}

function toggleCheckAll() {
  if (assignChecked.value.size === assignTapes.value.length) {
    assignChecked.value = new Set();
  } else {
    assignChecked.value = new Set(assignTapes.value.map((t) => t.name));
  }
  recomputeMappings();
}

function toggleCheck(name: string) {
  const next = new Set(assignChecked.value);
  if (next.has(name)) next.delete(name); else next.add(name);
  assignChecked.value = next;
  recomputeMappings();
}

function recomputeMappings() {
  const checked = Array.from(assignChecked.value);
  const slots = [...emptySlots.value];
  const usedSlots = new Set<number>();
  const m = new Map<string, number>();

  // First pass: keep valid existing mappings
  for (const name of checked) {
    const existing = slotMappings.value.get(name);
    if (existing != null && slots.includes(existing) && !usedSlots.has(existing)) {
      m.set(name, existing);
      usedSlots.add(existing);
    }
  }

  // Second pass: fill remaining tapes with unused slots in order
  let slotIdx = 0;
  for (const name of checked) {
    if (m.has(name)) continue;
    while (slotIdx < slots.length && usedSlots.has(slots[slotIdx])) slotIdx++;
    if (slotIdx < slots.length) {
      m.set(name, slots[slotIdx]);
      usedSlots.add(slots[slotIdx]);
      slotIdx++;
    }
  }

  slotMappings.value = m;
}

function setMapping(tape: string, slot: number | null) {
  if (slot == null) return;
  const next = new Map(slotMappings.value);
  // Check if slot already used by another tape
  for (const [ta, sl] of next) {
    if (sl === slot && ta !== tape) {
      message.warning(`槽位 ${slot + 1} 已分配给 ${ta}`);
      return;
    }
  }
  next.set(tape, slot);
  slotMappings.value = next;
}

const slotOpts = computed(() =>
  emptySlots.value.map((s) => ({ label: `Slot ${s + 1}`, value: s })),
);

async function handleBatchAssign() {
  if (!libraryName.value) return;
  const pairs: { tape: string; slot: number }[] = [];
  for (const [tape, slot] of slotMappings.value) {
    pairs.push({ tape, slot });
  }
  if (pairs.length === 0) { message.warning('请至少选择一盘磁带'); return; }
  assignLoading.value = true;
  try {
    await assignSlotBatch(libraryName.value, pairs);
    message.success(`${pairs.length} 盘磁带入槽完成`);
    assignChecked.value = new Set();
    slotMappings.value = new Map();
    await Promise.all([reloadDetail(), loadTapesForAssign()]);
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '入槽失败');
  } finally { assignLoading.value = false; }
}

// ── Tab 5: transport ──
// iSCSI
const iscsiConfig = ref<{
  iqn: string; portal_ip: string; portal_port: number;
  export_id: string; changer_sg: string; drive_sg: string[];
  allow_iscsi_exec: boolean; can_export: boolean; has_saved_export: boolean;
} | null>(null);
const iscsiAllowExec = ref(false);
const iscsiDryRun = ref(true);
const iscsiExecResult = ref<{ stdout: string; stderr: string; ok: boolean } | null>(null);
const iscsiBusy = ref(false);

// FC
const fcWwpn = ref('');
const fcWwnn = ref('');

// SCSI scan
const scanResult = ref<{ changer_sg: string | null; drive_sg: string[] } | null>(null);
const scanLoading = ref(false);
const driveSgInput = computed({
  get: () => iscsiConfig.value?.drive_sg.join(', ') ?? '',
  set: (val: string) => {
    if (iscsiConfig.value) {
      iscsiConfig.value = {
        ...iscsiConfig.value,
        drive_sg: val.split(',').map(s => s.trim()).filter(Boolean),
      };
    }
  },
});

const transportLoading = ref(false);

async function loadTransport() {
  transportLoading.value = true;
  try {
    const [cfg, fab] = await Promise.all([fetchIscsiConfig(), fetchFabric()]);
    iscsiAllowExec.value = cfg.allow_iscsi_exec;
    fcWwpn.value = fab.fc_wwpn ?? '';
    if (libraryName.value) {
      try {
        const defs = await fetchIscsiExportDefaults(libraryName.value);
        iscsiConfig.value = {
          iqn: defs.iqn,
          portal_ip: defs.portal_ip,
          portal_port: defs.portal_port,
          export_id: defs.export_id,
          changer_sg: defs.changer_sg ?? '',
          drive_sg: defs.drive_sg ?? [],
          allow_iscsi_exec: cfg.allow_iscsi_exec,
          can_export: defs.can_export,
          has_saved_export: defs.has_saved_export,
        };
      } catch {
        // export-defaults failed — keep cfg basics as fallback
        iscsiConfig.value = {
          iqn: cfg.iscsi_iqn ?? '',
          portal_ip: cfg.portal_ip_suggested,
          portal_port: cfg.portal_port_suggested,
          export_id: '',
          changer_sg: '',
          drive_sg: [],
          allow_iscsi_exec: cfg.allow_iscsi_exec,
          can_export: false,
          has_saved_export: false,
        };
      }
    }
  } catch { console.error('加载传输配置失败'); } finally { transportLoading.value = false; }
}

async function handleIscsiAllowExec(val: boolean) {
  try {
    await setIscsiAllowExec(val);
    iscsiAllowExec.value = val;
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '设置失败');
  }
}

async function handleIscsiExport() {
  if (!libraryName.value || !iscsiConfig.value) return;
  if (!iscsiAllowExec.value && !iscsiDryRun.value) {
    message.warning('请先开启「允许执行 vtladm-iscsi」');
    return;
  }
  iscsiBusy.value = true;
  try {
    const result = await libraryIscsiExport({
      library: libraryName.value,
      iqn: iscsiConfig.value.iqn,
      export_id: iscsiConfig.value.export_id,
      changer_sg: iscsiConfig.value.changer_sg,
      drive_sg: iscsiConfig.value.drive_sg,
      portal_ip: iscsiConfig.value.portal_ip,
      portal_port: iscsiConfig.value.portal_port,
      dry_run: iscsiDryRun.value,
    });
    iscsiExecResult.value = { stdout: result.stdout, stderr: result.stderr, ok: result.ok };
    if (result.ok) message.success('iSCSI 导出成功');
    else message.error('iSCSI 导出失败，查看执行结果');
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '导出失败');
  } finally { iscsiBusy.value = false; }
}

async function handleIscsiUnexport() {
  if (!libraryName.value || !iscsiConfig.value) return;
  iscsiBusy.value = true;
  try {
    const result = await libraryIscsiUnexport({
      library: libraryName.value,
      iqn: iscsiConfig.value.iqn,
      export_id: iscsiConfig.value.export_id,
      dry_run: iscsiDryRun.value,
    });
    iscsiExecResult.value = { stdout: result.stdout, stderr: result.stderr, ok: result.ok };
    if (result.ok) message.success('iSCSI 卸载成功');
    else message.error('iSCSI 卸载失败，查看执行结果');
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '卸载失败');
  } finally { iscsiBusy.value = false; }
}

async function handleScanSg() {
  if (!libraryName.value) return;
  scanLoading.value = true;
  try {
    const r = await scanTransportSg(libraryName.value);
    scanResult.value = { changer_sg: r.changer_sg, drive_sg: r.drive_sg };
    // Feed back to iscsi config (immutable update)
    if (iscsiConfig.value && r.changer_sg) {
      iscsiConfig.value = {
        ...iscsiConfig.value,
        changer_sg: r.changer_sg,
        drive_sg: r.drive_sg,
      };
    }
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '扫描失败');
  } finally { scanLoading.value = false; }
}

// ── Tab 6: reconcile ──
const reconcileResult = ref<{
  drift_count: number; fixes_applied: number; pull_updates: number;
  drifts: DriftItem[]; ok: boolean;
} | null>(null);
const reconcileLoading = ref(false);
const reconcileDone = ref(false);
const reconcileError = ref('');

async function runReconcile() {
  if (!libraryName.value) return;
  reconcileLoading.value = true;
  reconcileDone.value = false;
  reconcileError.value = '';
  try {
    const r = await robotReconcile(libraryName.value, true);
    reconcileResult.value = {
      drift_count: r.drift_count,
      fixes_applied: r.fixes_applied,
      pull_updates: r.pull_updates,
      drifts: r.drifts,
      ok: r.ok,
    };
    reconcileDone.value = true;
  } catch (e: unknown) {
    reconcileError.value = e instanceof Error ? e.message : '对账请求失败';
    reconcileDone.value = true;
  } finally { reconcileLoading.value = false; }
}

async function handleSync() {
  if (!libraryName.value) return;
  try {
    await robotSync(libraryName.value);
    message.success('Sync 完成');
    await runReconcile();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : 'Sync 失败');
  }
}

async function handleAutoAlign() {
  if (!libraryName.value) return;
  try {
    const r = await robotAutoAlign(libraryName.value);
    message.success(`Auto-Align 完成: ${r.evacuated} 疏散, ${r.fixes_applied} 修复`);
    await runReconcile();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : 'Auto-Align 失败');
  }
}

// ── lifecycle ──
async function reloadDetail() {
  if (!libraryName.value) return;
  try {
    detail.value = await fetchLibraryDetail(libraryName.value);
  } catch { console.error('API 调用失败'); }
}

// Handle tab change
async function handleTabChange(tab: string) {
  if (tab === 'shelves') await loadShelves();
  if (tab === 'transport') await loadTransport();
  if (tab === 'assign') await loadTapesForAssign();
  if (tab === 'reconcile' && !reconcileDone.value) await runReconcile();
}

onMounted(async () => {
  await libraryStore.loadLibraries();
  if (!libraryName.value) { loading.value = false; return; }
  await Promise.all([reloadDetail(), loadDensityLimits(), loadTapesForAssign(), loadShelves()]);
  loading.value = false;
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">库详情 · {{ libraryName || '——' }}</h2>

    <NSpin :show="loading">
      <template v-if="detail">
        <NTabs type="card" default-value="overview" @update:value="handleTabChange">
          <!-- ───── Tab 1: 库概览 ───── -->
          <NTabPane name="overview" tab="库概览">
            <NGrid :cols="4" :x-gap="12" :y-gap="12" style="margin-bottom: 16px">
              <NGi>
                <NCard size="small"><NStatistic label="驱动器" :value="detail.drives.length" /></NCard>
              </NGi>
              <NGi>
                <NCard size="small"><NStatistic label="数据槽位" :value="detail.changer.data_slots.length" /></NCard>
              </NGi>
              <NGi>
                <NCard size="small"><NStatistic label="磁带" :value="detail.tapes.length" /></NCard>
              </NGi>
              <NGi>
                <NCard size="small"><NStatistic label="空槽位" :value="emptySlotCount" /></NCard>
              </NGi>
            </NGrid>

            <NCard title="容量用量" size="small" style="margin-bottom: 16px">
              <div style="display:flex;justify-content:space-between;margin-bottom:6px">
                <span>总容量: {{ fmtBytes(totalCap) }}</span>
                <span>已用: {{ fmtBytes(totalUsed) }} ({{ usagePct }}%)</span>
                <span>可用: {{ fmtBytes(totalCap - totalUsed) }}</span>
              </div>
              <NProgress type="line" :percentage="usagePct" :height="20" :border-radius="4" />
            </NCard>

            <NCard title="驱动器状态" size="small" style="margin-bottom: 16px">
              <div v-for="d in detail.drives" :key="d.drive_id" style="margin-bottom:6px">
                <NTag :type="d.tape_name ? 'success' : 'default'" size="small">
                  Drive {{ d.drive_id }}: {{ d.tape_name || '(空)' }}
                </NTag>
                <span v-if="d.tape_barcode" style="font-size:12px;color:#999;margin-left:8px">{{ d.tape_barcode }}</span>
              </div>
            </NCard>

            <NCard title="槽位占用分布" size="small" style="margin-bottom:16px">
              <div style="display:flex;flex-wrap:wrap;gap:6px">
                <template v-for="s in detail.changer.data_slots" :key="s.label">
                  <NTag :type="s.tape_name ? 'info' : 'default'" size="small">
                    {{ s.label }}: {{ s.tape_name || '空' }}
                  </NTag>
                </template>
              </div>
            </NCard>

            <NCard v-if="detail.tapes.length > 0" title="磁带操作" size="small">
              <NDataTable
                :columns="[
                  { title: '名称', key: 'name', width: 140 },
                  { title: '条码', key: 'barcode', width: 120 },
                  { title: '容量', key: 'cap', width: 90, render: (r: TapeRow) => fmtBytes(r.capacity_bytes) },
                  { title: '位置', key: 'loc', width: 90, render: (r: TapeRow) => r.in_drive ? '驱动器中' : r.slot != null ? '槽位 '+(r.slot+1) : r.shelf_name ?? '——' },
                  {
                    title: '操作', key: 'actions', width: 180,
                    render(row: TapeRow) {
                      return h(NSpace, null, {
                        default: () => [
                          h(NPopconfirm, { onPositiveClick: () => handleTapeInit(row.name) }, {
                            trigger: () => h(NButton, { size: 'tiny' }, { default: () => '清空' }),
                            default: () => `确定清空磁带 ${row.name}？数据将不可恢复`,
                          }),
                          h(NPopconfirm, { onPositiveClick: () => handleTapeDelete(row.name) }, {
                            trigger: () => h(NButton, { size: 'tiny', type: 'error' }, { default: () => '删除' }),
                            default: () => `确定删除磁带 ${row.name}？`,
                          }),
                          h(NButton, { size: 'tiny', onClick: () => openShelfPlace(row) }, { default: () => '换架' }),
                        ],
                      });
                    },
                  },
                ]"
                :data="detail.tapes"
                :bordered="false"
                size="small"
                :max-height="300"
              />
            </NCard>
          </NTabPane>

          <!-- ───── Tab 2: 货架管理 ───── -->
          <NTabPane name="shelves" tab="货架管理">
            <NButton type="primary" size="small" style="margin-bottom:12px" @click="showCreateShelf = true">
              新建货架
            </NButton>
            <NCard size="small">
              <NDataTable
                :columns="shelfColumns"
                :data="shelves"
                :loading="shelvesLoading"
                :bordered="false"
                size="small"
              />
            </NCard>

            <NModal v-model:show="showCreateShelf" title="新建货架">
              <NCard style="width:360px" title="新建货架" :bordered="false">
                <NSpace vertical>
                  <span>货架名称</span>
                  <NInput v-model:value="newShelfName" placeholder="输入货架名称" />
                </NSpace>
                <NSpace justify="end" style="margin-top: 16px">
                  <NButton @click="showCreateShelf = false">取消</NButton>
                  <NButton type="primary" @click="handleCreateShelf">创建</NButton>
                </NSpace>
              </NCard>
            </NModal>

            <!-- Shelf place modal (shared) -->
            <NModal v-model:show="showShelfPlaceModal" title="磁带换架">
              <NCard style="width:380px" title="磁带换架" :bordered="false">
                <p style="margin-bottom:8px">磁带: <strong>{{ shelfPlaceTarget?.name }}</strong></p>
                <span>目标货架</span>
                <NSelect
                  v-model:value="shelfPlaceTargetShelf"
                  :options="tapeShelfOpts"
                  placeholder="未使用架"
                  clearable
                  style="margin-top:4px"
                />
                <NSpace justify="end" style="margin-top: 16px">
                  <NButton @click="showShelfPlaceModal = false">取消</NButton>
                  <NButton type="primary" @click="handleShelfPlaceExec">确认换架</NButton>
                </NSpace>
              </NCard>
            </NModal>
          </NTabPane>

          <!-- ───── Tab 3: 批量建带 ───── -->
          <NTabPane name="batch-create" tab="批量建带">
            <NCard title="批量创建磁带" size="small" style="max-width:560px">
              <NSpace vertical>
                <div>
                  <span>数量</span>
                  <NInputNumber v-model:value="batchCount" :min="1" :max="100" style="width:100%" />
                </div>
                <div style="font-size:13px;color:#666;margin-bottom:8px">
                  命名规则: <strong>{{ namePrefix }}01</strong> · {{ namePrefix }}02 · ...（库名自动前缀，确保唯一）
                </div>
                <div>
                  <span>密度格式</span>
                  <NSelect
                    v-model:value="selectedDensity"
                    :options="densityLimits.map(d => ({
                      label: `${d.label} (${d.min_human} ~ ${d.max_human})`,
                      value: d.code,
                    }))"
                    placeholder="选择密度格式（可选）"
                    clearable
                    style="width:100%"
                  />
                </div>
                <div>
                  <span>容量</span>
                  <NInput v-model:value="batchSize" placeholder="如 100G, 1T" />
                  <div v-if="selectedDensityInfo" style="font-size:12px;color:#666;margin-top:4px">
                    ℹ️ {{ selectedDensityInfo.label }} 容量范围: {{ selectedDensityInfo.min_human }} ~ {{ selectedDensityInfo.max_human }}
                  </div>
                  <div v-if="capacityError" style="font-size:12px;color:#d03050;margin-top:4px">
                    {{ capacityError }}
                  </div>
                </div>
                <div>
                  <span>目标货架</span>
                  <NSelect
                    v-model:value="batchShelf"
                    :options="tapeShelfOpts"
                    placeholder="(默认架)"
                    clearable
                    style="width:100%"
                  />
                </div>
                <div style="font-size:13px;color:#666">
                  预览: {{ namePreview }}
                </div>
              </NSpace>
              <NSpace justify="end" style="margin-top: 16px">
                <NButton type="primary" :loading="creating" :disabled="capacityError != null" @click="handleBatchCreate">
                  批量建带
                </NButton>
              </NSpace>
              <div style="font-size:12px;color:#999;margin-top:8px">
                条码由系统自动生成，保证唯一性
              </div>
            </NCard>
          </NTabPane>

          <!-- ───── Tab 4: 磁带入槽 ───── -->
          <NTabPane name="assign" tab="磁带入槽">
            <div style="font-size:13px;color:#666;margin-bottom:12px">
              可用空槽位: {{ emptySlots.length }} 个
              <template v-if="emptySlots.length > 0">
                ({{ emptySlots.slice(0, 10).join(', ') }}{{ emptySlots.length > 10 ? '...' : '' }})
              </template>
            </div>

            <template v-if="assignTapes.length > 0">
              <NCard size="small">
                <div style="margin-bottom:8px">
                  <NCheckbox
                    :checked="assignChecked.size === assignTapes.length"
                    @update:checked="toggleCheckAll"
                  >
                    全选 · 货架上的磁带 ({{ assignTapes.length }} 盘)
                  </NCheckbox>
                </div>
                <div
                  v-for="tape in assignTapes"
                  :key="tape.name"
                  style="display:flex;align-items:center;gap:12px;padding:6px 0;border-bottom:1px solid #f0f0f0"
                >
                  <NCheckbox
                    :checked="assignChecked.has(tape.name)"
                    @update:checked="() => toggleCheck(tape.name)"
                  />
                  <span style="width:160px;font-weight:500">{{ tape.name }}</span>
                  <span style="width:100px;font-size:12px;color:#999">{{ tape.shelf_name || '——' }}</span>
                  <span style="font-size:12px;color:#666">入槽</span>
                  <NSelect
                    :value="assignChecked.has(tape.name) ? (slotMappings.get(tape.name) ?? null) : null"
                    :options="slotOpts"
                    size="tiny"
                    style="width:110px"
                    :disabled="!assignChecked.has(tape.name)"
                    placeholder="——"
                    @update:value="(v: number | null) => v != null && setMapping(tape.name, v)"
                  />
                </div>
              </NCard>
              <div style="margin-top:12px;font-size:13px;color:#666">
                已选 {{ assignChecked.size }} 盘 · 默认按空槽位顺序分配，可逐行手动调整
              </div>
              <NButton
                type="primary"
                :loading="assignLoading"
                :disabled="assignChecked.size === 0"
                style="margin-top:12px"
                @click="handleBatchAssign"
              >
                批量入槽 ({{ assignChecked.size }})
              </NButton>
            </template>
            <NEmpty v-else description="暂无可入槽的磁带（需先在货架上创建磁带）" />
          </NTabPane>

          <!-- ───── Tab 5: 传输配置 ───── -->
          <NTabPane name="transport" tab="传输配置">
            <NSpin :show="transportLoading">
              <!-- transport status -->
              <NCard title="当前传输状态" size="small" style="margin-bottom:16px">
                <div style="font-size:13px">
                  <div>vtladm-iscsi 路径: <code>{{ iscsiConfig ? '已配置' : '——' }}</code></div>
                </div>
              </NCard>

              <!-- iSCSI -->
              <NCard title="iSCSI 导出" size="small" style="margin-bottom:16px">
                <template v-if="iscsiConfig">
                  <NSpace vertical style="width:100%">
                    <div>
                      <span>IQN</span>
                      <NInput v-model:value="iscsiConfig.iqn" placeholder="iqn..."/>
                    </div>
                    <NGrid :cols="2" :x-gap="12">
                      <NGi>
                        <span>Portal IP</span>
                        <NInput v-model:value="iscsiConfig.portal_ip" placeholder="0.0.0.0" />
                      </NGi>
                      <NGi>
                        <span>Portal Port</span>
                        <NInputNumber v-model:value="iscsiConfig.portal_port" :min="1" :max="65535" style="width:100%" />
                      </NGi>
                    </NGrid>
                    <div>
                      <span>导出 ID</span>
                      <NInput v-model:value="iscsiConfig.export_id" placeholder="export_id" />
                    </div>
                    <div>
                      <span>Changer SG</span>
                      <NInput v-model:value="iscsiConfig.changer_sg" placeholder="/dev/sgX" />
                    </div>
                    <div>
                      <span>Drive SG (逗号分隔)</span>
                      <NInput v-model:value="driveSgInput" placeholder="/dev/sgY, /dev/sgZ" />
                    </div>
                    <div style="font-size:12px;color:#999">
                      <template v-if="iscsiConfig.has_saved_export">已保存导出记录</template>
                      <template v-else-if="iscsiConfig.can_export">可导出: {{ iscsiConfig.drive_sg.length }} drives</template>
                      <template v-else>不可导出</template>
                    </div>
                  </NSpace>

                  <NGrid :cols="2" :x-gap="12" style="margin-top:12px">
                    <NGi>
                      <div style="font-size:13px;margin-bottom:4px">允许执行 vtladm-iscsi</div>
                      <NSwitch :value="iscsiAllowExec" @update:value="handleIscsiAllowExec" />
                    </NGi>
                    <NGi>
                      <div style="font-size:13px;margin-bottom:4px">仅 dry-run（预览不执行）</div>
                      <NSwitch v-model:value="iscsiDryRun" />
                    </NGi>
                  </NGrid>

                  <NSpace style="margin-top:12px">
                    <NButton type="primary" :loading="iscsiBusy" @click="handleIscsiExport">导出库到 iSCSI</NButton>
                    <NButton :loading="iscsiBusy" @click="handleIscsiUnexport">卸载 iSCSI 导出</NButton>
                  </NSpace>

                  <template v-if="iscsiExecResult">
                    <NCard size="small" title="执行结果" style="margin-top:12px">
                      <div style="font-size:12px">
                        <div><strong>状态:</strong> {{ iscsiExecResult.ok ? '✓ 成功' : '✗ 失败' }}</div>
                        <pre style="max-height:200px;overflow:auto;background:#f5f5f5;padding:8px;margin:4px 0">{{ iscsiExecResult.stdout }}</pre>
                        <div v-if="iscsiExecResult.stderr" style="color:#d03050">
                          <strong>stderr:</strong>
                          <pre style="max-height:120px;overflow:auto;background:#fff0f0;padding:8px;margin:4px 0">{{ iscsiExecResult.stderr }}</pre>
                        </div>
                      </div>
                    </NCard>
                  </template>
                </template>
                <div v-else style="color:#999;text-align:center;padding:16px">
                  无法获取 iSCSI 配置。请确认库已创建且有驱动器。
                </div>
              </NCard>

              <!-- FC -->
              <NCard title="FC 配置" size="small" style="margin-bottom:16px">
                <NSpace vertical style="width:100%">
                  <div>
                    <span>WWPN</span>
                    <NInput v-model:value="fcWwpn" placeholder="10:00:00:90:fa:xx:xx:xx" />
                  </div>
                  <div>
                    <span>WWNN</span>
                    <NInput v-model:value="fcWwnn" placeholder="20:00:00:90:fa:xx:xx:xx" />
                  </div>
                  <div style="font-size:12px;color:#999">
                    FC 配置需系统级 FC target 驱动支持。当前为前端预留编辑能力。
                  </div>
                </NSpace>
              </NCard>

              <!-- SCSI Scan -->
              <NCard title="SCSI 设备扫描" size="small">
                <NButton size="small" :loading="scanLoading" @click="handleScanSg">扫描 SCSI 设备</NButton>
                <div v-if="scanResult" style="margin-top:8px;font-size:13px">
                  <div>Changer: {{ scanResult.changer_sg || '未检测到' }}</div>
                  <div>Drives: {{ scanResult.drive_sg.length > 0 ? scanResult.drive_sg.join(', ') : '未检测到' }}</div>
                </div>
              </NCard>
            </NSpin>
          </NTabPane>

          <!-- ───── Tab 6: 对账 ───── -->
          <NTabPane name="reconcile" tab="对账">
            <NSpin :show="reconcileLoading">
              <template v-if="reconcileDone && reconcileResult">
                <template v-if="reconcileResult.drift_count === 0">
                  <NCard size="small">
                    <div style="text-align:center;padding:20px;color:#18a058;font-size:16px">
                      ✓ DB 与内核 inventory 一致
                    </div>
                  </NCard>
                </template>
                <template v-else>
                  <NCard size="small" title="发现漂移" style="margin-bottom:12px">
                    <p>漂移项: {{ reconcileResult.drift_count }} | 修复: {{ reconcileResult.fixes_applied }} | Pull: {{ reconcileResult.pull_updates }}</p>
                  </NCard>
                  <NCard size="small">
                    <NDataTable
                      :columns="[
                        { title: '磁带', key: 'tape', width: 150 },
                        { title: 'DB 位置', key: 'db', width: 100 },
                        { title: '内核位置', key: 'kernel', width: 100 },
                        {
                          title: '状态', key: 'status', width: 80,
                          render: (r: DriftItem) => {
                            if (r.db && r.kernel && r.db !== r.kernel) return h(NTag, { type: 'warning', size: 'tiny' }, { default: () => '漂移' });
                            if (r.db && !r.kernel) return h(NTag, { type: 'info', size: 'tiny' }, { default: () => '仅DB' });
                            if (!r.db && r.kernel) return h(NTag, { type: 'error', size: 'tiny' }, { default: () => '仅内核' });
                            return h(NTag, { type: 'success', size: 'tiny' }, { default: () => '一致' });
                          },
                        },
                      ]"
                      :data="reconcileResult.drifts"
                      :bordered="false"
                      size="small"
                    />
                    <NSpace style="margin-top:12px">
                      <NButton size="small" @click="handleSync">Sync DB</NButton>
                      <NButton size="small" @click="handleAutoAlign">Auto-Align</NButton>
                      <NButton size="small" @click="runReconcile">重新对账</NButton>
                    </NSpace>
                  </NCard>
                </template>
              </template>
              <template v-else-if="reconcileDone && reconcileError">
                <NCard size="small">
                  <div style="text-align:center;padding:20px;color:#d03050">
                    ✗ 对账失败: {{ reconcileError }}
                  </div>
                  <div style="text-align:center;margin-top:8px">
                    <NButton size="small" @click="runReconcile">重试</NButton>
                  </div>
                </NCard>
              </template>
              <div v-else style="color:#999;text-align:center;padding:20px">正在对账...</div>
            </NSpin>
          </NTabPane>
        </NTabs>
      </template>
      <div v-else style="color:#999;text-align:center;padding:40px">库不存在或已删除</div>
    </NSpin>
  </div>
</template>
