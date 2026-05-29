<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { NDataTable, NButton, NSpace, NSelect, useMessage } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchTapes } from '@/api/tapes';
import { fetchEmptySlots } from '@/api/other';
import { assignSlot } from '@/api/tapes';
import type { TapeRow } from '@/api/types';

const message = useMessage();
const libraryStore = useLibraryStore();

const tapes = ref<TapeRow[]>([]);
const emptySlots = ref<number[]>([]);
const selectedTape = ref<string | null>(null);
const selectedSlot = ref<number | null>(null);

const tapeOpts = computed(() =>
  tapes.value
    .filter((t) => !t.in_drive && t.shelf_name != null)
    .map((t) => ({ label: `${t.name} (${t.shelf_name || '——'})`, value: t.name })),
);

const slotOpts = computed(() =>
  emptySlots.value.map((s) => ({ label: `Slot ${s}`, value: s })),
);

async function load() {
  if (!libraryStore.currentLibrary) return;
  try {
    const [t, s] = await Promise.all([
      fetchTapes(libraryStore.currentLibrary, 0, 5000),
      fetchEmptySlots(libraryStore.currentLibrary),
    ]);
    tapes.value = t.tapes;
    emptySlots.value = s.empty_slots;
  } catch {
    // ignore
  }
}

async function handleAssign() {
  if (!libraryStore.currentLibrary || !selectedTape.value || selectedSlot.value == null) {
    message.warning('请选择磁带和目标槽位');
    return;
  }
  try {
    await assignSlot(libraryStore.currentLibrary, selectedTape.value, selectedSlot.value);
    message.success(`${selectedTape.value} → Slot ${selectedSlot.value}`);
    selectedTape.value = null;
    selectedSlot.value = null;
    await load();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '入槽失败');
  }
}

onMounted(load);
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">磁带入槽</h2>

    <NSpace vertical style="max-width: 500px">
      <div>
        <span>选择磁带</span>
        <NSelect v-model:value="selectedTape" :options="tapeOpts" placeholder="选择货架上的磁带" filterable />
      </div>
      <div>
        <span>目标槽位</span>
        <NSelect v-model:value="selectedSlot" :options="slotOpts" placeholder="选择空槽位" />
      </div>
      <NButton type="primary" @click="handleAssign">确认入槽</NButton>
      <div style="font-size: 13px; color: #999">
        可用空槽位: {{ emptySlots.length }} 个
        ({{ emptySlots.slice(0, 10).join(', ') }}{{ emptySlots.length > 10 ? '...' : '' }})
      </div>
    </NSpace>
  </div>
</template>
