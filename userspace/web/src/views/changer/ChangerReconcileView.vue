<script setup lang="ts">
import { ref, onMounted, h } from 'vue';
import { NCard, NDataTable, NButton, NSpace, NSpin, NTag, useMessage } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { robotReconcile, robotAutoAlign, robotSync } from '@/api/changer';
import * as libApi from '@/api/libraries';
import type { DriftItem, LibraryDetailResponse } from '@/api/types';

const message = useMessage();
const libraryStore = useLibraryStore();

const loading = ref(false);
const detail = ref<LibraryDetailResponse | null>(null);
const drifts = ref<DriftItem[]>([]);
const driftCount = ref(0);
const pullUpdates = ref(0);
const lastOp = ref('');

async function loadInventory() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const d = await libApi.fetchLibraryDetail(libraryStore.currentLibrary);
    detail.value = d;
  } catch {
    // ignore
  }
  loading.value = false;
}

async function handleReconcile() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const r = await robotReconcile(libraryStore.currentLibrary);
    drifts.value = r.drifts;
    driftCount.value = r.drift_count;
    pullUpdates.value = r.pull_updates;
    lastOp.value = r.pull_updates > 0 ? `从内核拉取 ${r.pull_updates} 条记录` : '无漂移或仅漂移检测';
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '对账失败');
  }
  loading.value = false;
}

async function handlePull() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const r = await robotReconcile(libraryStore.currentLibrary, true);
    drifts.value = r.drifts;
    driftCount.value = r.drift_count;
    pullUpdates.value = r.pull_updates;
    lastOp.value = `Pull 完成: 更新 ${r.pull_updates} 条, 剩余漂移 ${r.drift_count}`;
    await loadInventory();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : 'Pull 失败');
  }
  loading.value = false;
}

async function handleAutoAlign() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const r = await robotAutoAlign(libraryStore.currentLibrary);
    lastOp.value = `Auto-Align: 撤出 ${r.evacuated} 条, 修复 ${r.fixes_applied} 条, Pull ${r.pull_updates} 条, 剩余 ${r.drifts_remaining} 条`;
    await loadInventory();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : 'Auto-Align 失败');
  }
  loading.value = false;
}

async function handleSync() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const r = await robotSync(libraryStore.currentLibrary);
    lastOp.value = `Sync-DB: 更新 ${r.tapes_updated} 条`;
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : 'Sync 失败');
  }
  loading.value = false;
}

const driftCols: DataTableColumns<DriftItem> = [
  { title: '磁带', key: 'tape', width: 150 },
  { title: 'DB 位置', key: 'db', width: 120, render: (r: DriftItem) => r.db || '——' },
  { title: '内核位置', key: 'kernel', width: 120, render: (r: DriftItem) => r.kernel || '——' },
  {
    title: '状态',
    key: 'status',
    width: 100,
    render(r) {
      if (!r.db && !r.kernel) return '——';
      if (r.db && r.kernel && r.db !== r.kernel) return h(NTag, { type: 'warning', size: 'small' }, { default: () => '漂移' });
      if (r.db && !r.kernel) return h(NTag, { type: 'error', size: 'small' }, { default: () => '仅 DB' });
      if (!r.db && r.kernel) return h(NTag, { type: 'info', size: 'small' }, { default: () => '仅内核' });
      return h(NTag, { type: 'success', size: 'small' }, { default: () => '一致' });
    },
  },
];

onMounted(async () => {
  await loadInventory();
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">机械手对账 (Inventory Reconciliation)</h2>

    <NSpace style="margin-bottom: 12px">
      <NButton size="small" @click="loadInventory()">刷新 Inventory</NButton>
      <NButton size="small" type="primary" @click="handlePull()">Pull from Kernel</NButton>
      <NButton size="small" @click="handleAutoAlign()">Auto-Align</NButton>
      <NButton size="small" @click="handleSync()">Sync DB</NButton>
    </NSpace>

    <NSpin :show="loading">
      <NCard size="small" style="margin-bottom: 16px">
        <div v-if="detail">
          库: <strong>{{ detail.library.name }}</strong>
          · 驱动器: {{ detail.drives.length }}
          · 槽位: {{ detail.changer.data_slots.length }}
          · 磁带: {{ detail.tapes.length }}
        </div>
        <div v-if="detail?.changer.source" style="font-size: 12px; color: #999; margin-top: 4px">
          Inventory 数据来源: {{ detail.changer.source }}
        </div>
      </NCard>

      <NCard v-if="drifts.length > 0" title="漂移列表" size="small" style="margin-bottom: 16px">
        <NDataTable :columns="driftCols" :data="drifts" :bordered="false" size="small" :max-height="400" />
        <div style="margin-top: 8px; font-size: 13px; color: #f0a020">
          漂移: {{ driftCount }} 条
        </div>
      </NCard>

      <NCard v-else size="small" style="margin-bottom: 16px">
        <div style="color: #18a058; text-align: center; padding: 20px">
          ✓ DB 与内核 inventory 一致 (或未执行对账)
        </div>
      </NCard>

      <NCard v-if="lastOp" title="操作结果" size="small">
        <div style="font-size: 13px">{{ lastOp }}</div>
      </NCard>
    </NSpin>
  </div>
</template>
