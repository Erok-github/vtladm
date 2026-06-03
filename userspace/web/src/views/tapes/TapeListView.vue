<script setup lang="ts">
import { ref, onMounted, computed, h } from 'vue';
import {
  NCard, NDataTable, NButton, NSpace, NTag, NProgress, NSelect,
  NModal, NInput, NInputNumber, NSwitch, NForm, NFormItem, useMessage,
  NPopconfirm, NDropdown, NText,
} from 'naive-ui';
import type { DataTableColumns, DropdownOption } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchTapes, createTape, deleteTape, initTape, createTapeAutoBatch, assignSlot, shelfPlace } from '@/api/tapes';
import { fetchEmptySlots } from '@/api/other';
import { fetchShelves } from '@/api/shelves';
import type { TapeRow, ShelfRow } from '@/api/types';

const message = useMessage();
const libraryStore = useLibraryStore();

const tapes = ref<TapeRow[]>([]);
const total = ref(0);
const offset = ref(0);
const limit = ref(100);
const loading = ref(false);
const selectedKeys = ref<Set<string>>(new Set());

// Create tape modal
const showCreate = ref(false);
const createName = ref('');
const createSize = ref('10G');
const createShelf = ref<string | null>(null);
const shelfOptions = ref<{ label: string; value: string }[]>([]);

// Batch create modal
const showBatchCreate = ref(false);
const batchCount = ref(5);
const batchPrefix = ref('TAPE');
const batchSize = ref('10G');
const batchShelf = ref<string | null>(null);

// Assign slot modal
const showAssignSlot = ref(false);
const assignTape = ref<TapeRow | null>(null);
const assignSlotNum = ref<number | null>(null);
const emptySlots = ref<number[]>([]);

// Shelf place modal
const showShelfPlace = ref(false);
const shelfPlaceTape = ref<TapeRow | null>(null);
const shelfPlaceTarget = ref<string | null>(null);

// Stat
const totalCapacity = computed(() => tapes.value.reduce((s, t) => s + t.capacity_bytes, 0));
const totalUsed = computed(() => tapes.value.reduce((s, t) => s + t.used_bytes, 0));

function fmtBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024 * 1024)).toFixed(1) + ' TB';
  if (b >= 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
  if (b >= 1024 * 1024) return (b / (1024 * 1024)).toFixed(0) + ' MB';
  return b + ' B';
}

function pct(used: number, cap: number): number {
  return cap > 0 ? Math.round((used / cap) * 100) : 0;
}

async function loadTapes() {
  if (!libraryStore.currentLibrary) return;
  loading.value = true;
  try {
    const data = await fetchTapes(libraryStore.currentLibrary, offset.value, limit.value);
    tapes.value = data.tapes;
    total.value = data.total;
  } catch {
    message.error('加载磁带列表失败');
  } finally {
    loading.value = false;
  }
}

async function loadShelves() {
  if (!libraryStore.currentLibrary) return;
  try {
    const data = await fetchShelves(libraryStore.currentLibrary);
    shelfOptions.value = data.shelves.map((s) => ({ label: s.name, value: s.name }));
  } catch {
    // ignore
  }
}

async function handleCreate() {
  if (!libraryStore.currentLibrary || !createName.value.trim()) return;
  try {
    await createTape(libraryStore.currentLibrary, createName.value.trim(), createSize.value, createShelf.value ?? undefined);
    message.success(`磁带 ${createName.value} 创建成功`);
    showCreate.value = false;
    createName.value = '';
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '创建失败');
  }
}

async function handleBatchCreate() {
  if (!libraryStore.currentLibrary) return;
  try {
    const result = await createTapeAutoBatch(
      libraryStore.currentLibrary,
      batchCount.value,
      batchSize.value,
      batchShelf.value ?? undefined,
    );
    message.success(`批量创建 ${result.count} 盘磁带完成`);
    showBatchCreate.value = false;
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '批量创建失败');
  }
}

async function handleDelete(name: string) {
  if (!libraryStore.currentLibrary) return;
  try {
    await deleteTape(libraryStore.currentLibrary, name);
    message.success(`磁带 ${name} 已删除`);
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '删除失败');
  }
}

async function handleInit(name: string) {
  if (!libraryStore.currentLibrary) return;
  try {
    await initTape(libraryStore.currentLibrary, name);
    message.success(`磁带 ${name} 已清空`);
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '清空失败');
  }
}

async function handleAssignSlotExec() {
  if (!libraryStore.currentLibrary || !assignTape.value || assignSlotNum.value == null) return;
  try {
    await assignSlot(libraryStore.currentLibrary, assignTape.value.name, assignSlotNum.value);
    message.success(`磁带 ${assignTape.value.name} 已入槽 ${assignSlotNum.value}`);
    showAssignSlot.value = false;
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '入槽失败');
  }
}

async function handleShelfPlaceExec() {
  if (!libraryStore.currentLibrary || !shelfPlaceTape.value) return;
  try {
    await shelfPlace(libraryStore.currentLibrary, shelfPlaceTape.value.name, shelfPlaceTarget.value ?? undefined);
    message.success(`磁带 ${shelfPlaceTape.value.name} 已移至货架`);
    showShelfPlace.value = false;
    await loadTapes();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '换架失败');
  }
}

async function openAssignSlot(tape: TapeRow) {
  assignTape.value = tape;
  if (!libraryStore.currentLibrary) return;
  try {
    const data = await fetchEmptySlots(libraryStore.currentLibrary);
    emptySlots.value = data.empty_slots;
    if (data.empty_slots.length > 0) assignSlotNum.value = data.empty_slots[0];
  } catch {
    emptySlots.value = [];
  }
  showAssignSlot.value = true;
}

function openShelfPlace(tape: TapeRow) {
  shelfPlaceTape.value = tape;
  shelfPlaceTarget.value = null;
  showShelfPlace.value = true;
}

function locationLabel(t: TapeRow): string {
  if (t.in_drive) return '驱动器中';
  if (t.slot != null) return `槽位 ${t.slot}`;
  if (t.shelf_name) return t.shelf_name;
  return '——';
}

function rowKey(row: TapeRow): string {
  return row.name;
}

const columns: DataTableColumns<TapeRow> = [
  { type: 'selection' },
  {
    title: '磁带名称',
    key: 'name',
    width: 180,
    render(row) {
      return [
        h('div', { style: 'font-weight:600' }, row.name),
        h('div', { style: 'font-size:12px;color:#999' }, row.barcode),
      ];
    },
  },
  {
    title: '容量',
    key: 'capacity_bytes',
    width: 150,
    render(row) {
      return [
        h('div', fmtBytes(row.capacity_bytes)),
        h(NProgress, {
          type: 'line',
          percentage: pct(row.used_bytes, row.capacity_bytes),
          height: 6,
          borderRadius: 3,
          showText: false,
          style: 'margin-top:4px;max-width:120px',
        }),
      ];
    },
  },
  { title: '已用', key: 'used_bytes', width: 90, render: (r: TapeRow) => fmtBytes(r.used_bytes) },
  { title: '密度', key: 'density', width: 70, render: () => '——' },
  { title: '压缩', key: 'compression', width: 60, render: () => '——' },
  {
    title: '位置',
    key: 'location',
    width: 110,
    render(row) {
      return locationLabel(row);
    },
  },
  {
    title: '操作',
    key: 'actions',
    width: 200,
    render(row) {
      const opts: DropdownOption[] = [
        { label: '磁带入槽', key: 'assign' },
        { label: '换架', key: 'shelf-place' },
        { label: '清空 (init)', key: 'init' },
        { label: '删除', key: 'delete' },
      ];
      return h(
        NDropdown,
        {
          trigger: 'click',
          options: opts,
          onSelect: (key: string) => {
            if (key === 'assign') openAssignSlot(row);
            else if (key === 'shelf-place') openShelfPlace(row);
            else if (key === 'init') handleInit(row.name);
            else if (key === 'delete') handleDelete(row.name);
          },
        },
        { default: () => h(NButton, { size: 'tiny' }, { default: () => '···' }) },
      );
    },
  },
];

onMounted(async () => {
  await loadShelves();
  await loadTapes();
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">磁带管理</h2>

    <NSpace style="margin-bottom: 12px">
      <NButton type="primary" size="small" @click="showCreate = true">建带</NButton>
      <NButton size="small" @click="showBatchCreate = true">批量建带</NButton>
    </NSpace>

    <NCard size="small">
      <NDataTable
        :columns="columns"
        :data="tapes"
        :loading="loading"
        :row-key="rowKey"
        :bordered="false"
        :single-line="false"
        size="small"
        virtual-scroll
        :max-height="600"
      />
      <div style="margin-top: 8px; font-size: 13px; color: #999">
        共 {{ total }} 条 · 总容量 {{ fmtBytes(totalCapacity) }} · 已用 {{ fmtBytes(totalUsed) }}
      </div>
    </NCard>

    <!-- Create Tape Modal -->
    <NModal v-model:show="showCreate" title="创建磁带">
      <NCard style="width: 480px" title="新建磁带" :bordered="false">
        <NSpace vertical>
          <span>名称</span>
          <NInput v-model:value="createName" placeholder="磁带名称" />
          <span>容量</span>
          <NInput v-model:value="createSize" placeholder="如 100G, 1T" />
          <span>货架</span>
          <NSelect v-model:value="createShelf" :options="shelfOptions" placeholder="(默认架)" clearable />
        </NSpace>
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showCreate = false">取消</NButton>
          <NButton type="primary" @click="handleCreate">创建</NButton>
        </NSpace>
      </NCard>
    </NModal>

    <!-- Batch Create Modal -->
    <NModal v-model:show="showBatchCreate" title="批量建带">
      <NCard style="width: 480px" title="批量建带" :bordered="false">
        <NSpace vertical>
          <span>数量</span>
          <NInputNumber v-model:value="batchCount" :min="1" :max="100" />
          <span>名称前缀</span>
          <NInput v-model:value="batchPrefix" placeholder="TAPE" />
          <span>容量</span>
          <NInput v-model:value="batchSize" placeholder="如 100G" />
          <span>货架</span>
          <NSelect v-model:value="batchShelf" :options="shelfOptions" placeholder="(默认架)" clearable />
        </NSpace>
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showBatchCreate = false">取消</NButton>
          <NButton type="primary" @click="handleBatchCreate">创建</NButton>
        </NSpace>
      </NCard>
    </NModal>

    <!-- Assign Slot Modal -->
    <NModal v-model:show="showAssignSlot" title="磁带入槽">
      <NCard style="width: 400px" title="磁带入槽" :bordered="false">
        <p>磁带: <strong>{{ assignTape?.name }}</strong></p>
        <span>目标槽位</span>
        <NSelect
          v-model:value="assignSlotNum"
          :options="emptySlots.map(s => ({ label: `Slot ${s}`, value: s }))"
          style="margin-top: 4px"
        />
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showAssignSlot = false">取消</NButton>
          <NButton type="primary" @click="handleAssignSlotExec">确认入槽</NButton>
        </NSpace>
      </NCard>
    </NModal>

    <!-- Shelf Place Modal -->
    <NModal v-model:show="showShelfPlace" title="磁带换架">
      <NCard style="width: 400px" title="磁带换架" :bordered="false">
        <p>磁带: <strong>{{ shelfPlaceTape?.name }}</strong></p>
        <span>目标货架</span>
        <NSelect
          v-model:value="shelfPlaceTarget"
          :options="shelfOptions"
          style="margin-top: 4px"
          clearable
          placeholder="未使用架"
        />
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showShelfPlace = false">取消</NButton>
          <NButton type="primary" @click="handleShelfPlaceExec">确认换架</NButton>
        </NSpace>
      </NCard>
    </NModal>
  </div>
</template>
