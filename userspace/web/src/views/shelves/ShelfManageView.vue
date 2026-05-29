<script setup lang="ts">
import { ref, onMounted, h } from 'vue';
import { NCard, NButton, NSpace, NModal, NInput, NTag, NDataTable, useMessage, NPopconfirm } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchShelves, createShelf, deleteShelf } from '@/api/shelves';
import type { ShelfRow } from '@/api/types';

const message = useMessage();
const libraryStore = useLibraryStore();

const shelves = ref<ShelfRow[]>([]);
const showCreate = ref(false);
const newShelfName = ref('');

const columns: DataTableColumns<ShelfRow> = [
  { title: '名称', key: 'name', width: 160 },
  { title: 'ID', key: 'id', width: 60 },
  {
    title: '类型',
    key: 'type',
    width: 80,
    render: (r: ShelfRow) => r.is_default_unused ? '默认' : '自定义',
  },
  {
    title: '操作',
    key: 'actions',
    width: 100,
    render(row) {
      if (row.is_default_unused) return null;
      return h(
        NPopconfirm,
        { onPositiveClick: () => handleDelete(row.name) },
        {
          trigger: () => h(NButton, { size: 'tiny', type: 'error' }, { default: () => '删除' }),
          default: () => `确定删除货架 ${row.name}？架上须无磁带`,
        },
      );
    },
  },
];

async function load() {
  if (!libraryStore.currentLibrary) return;
  try {
    const data = await fetchShelves(libraryStore.currentLibrary);
    shelves.value = data.shelves;
  } catch {
    // ignore
  }
}

async function handleCreate() {
  if (!libraryStore.currentLibrary || !newShelfName.value.trim()) return;
  try {
    await createShelf(libraryStore.currentLibrary, newShelfName.value.trim());
    message.success(`货架 ${newShelfName.value} 创建成功`);
    showCreate.value = false;
    newShelfName.value = '';
    await load();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '创建失败');
  }
}

async function handleDelete(name: string) {
  if (!libraryStore.currentLibrary) return;
  try {
    await deleteShelf(libraryStore.currentLibrary, name);
    message.success(`货架 ${name} 已删除`);
    await load();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '删除失败');
  }
}

onMounted(load);
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">货架管理</h2>
    <NButton type="primary" size="small" style="margin-bottom: 12px" @click="showCreate = true">新建货架</NButton>

    <NCard size="small">
      <NDataTable :columns="columns" :data="shelves" :bordered="false" size="small" />
    </NCard>

    <NModal v-model:show="showCreate" title="新建货架">
      <NCard style="width: 360px" title="新建货架" :bordered="false">
        <NSpace vertical>
          <span>名称</span>
          <NInput v-model:value="newShelfName" placeholder="货架名称" />
        </NSpace>
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showCreate = false">取消</NButton>
          <NButton type="primary" @click="handleCreate">创建</NButton>
        </NSpace>
      </NCard>
    </NModal>
  </div>
</template>
