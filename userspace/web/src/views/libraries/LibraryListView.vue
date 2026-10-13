<script setup lang="ts">
import { ref, onMounted, h } from 'vue';
import { useRouter } from 'vue-router';
import { NDataTable, NButton, NSpace, NModal, NCard, NInput, NInputNumber, NText, useMessage, NPopconfirm } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { createLibrary, deleteLibrary } from '@/api/libraries';
import type { LibraryRow } from '@/api/types';

const router = useRouter();
const message = useMessage();
const libraryStore = useLibraryStore();

const showCreate = ref(false);
const newName = ref('');
const newDrives = ref(2);
const newSlots = ref(10);

const columns: DataTableColumns<LibraryRow> = [
  { title: '名称', key: 'name', width: 160 },
  { title: 'ID', key: 'id', width: 60 },
  { title: '类型', key: 'type', width: 80, render: (r: LibraryRow) => r.is_offline_storage ? '离线' : '在线' },
  { title: '创建时间', key: 'created_at', width: 180 },
  {
    title: '操作',
    key: 'actions',
    width: 160,
    render(row) {
      return h(NSpace, null, {
        default: () => [
          h(NButton, { size: 'tiny', onClick: () => router.push(`/libraries/${row.id}`) }, { default: () => '详情' }),
          !row.is_offline_storage
            ? h(
                NPopconfirm,
                { onPositiveClick: () => handleDelete(row.name) },
                {
                  trigger: () => h(NButton, { size: 'tiny', type: 'error' }, { default: () => '删除' }),
                  default: () => `确定删除库 ${row.name}？`,
                },
              )
            : null,
        ],
      });
    },
  },
];

async function handleCreate() {
  if (!newName.value.trim()) return;
  try {
    await createLibrary(newName.value.trim(), newDrives.value, newSlots.value);
    message.success(`磁带库 ${newName.value} 创建成功`);
    showCreate.value = false;
    newName.value = '';
    await libraryStore.loadLibraries();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '创建失败');
  }
}

async function handleDelete(name: string) {
  try {
    await deleteLibrary(name);
    message.success(`磁带库 ${name} 已删除`);
    await libraryStore.loadLibraries();
  } catch (e: unknown) {
    message.error(e instanceof Error ? e.message : '删除失败');
  }
}

onMounted(async () => {
  await libraryStore.loadLibraries();
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 12px; font-size: 20px;">磁带库管理</h2>
    <NButton type="primary" size="small" style="margin-bottom: 12px" @click="showCreate = true">新建库</NButton>

    <NCard size="small">
      <NDataTable :columns="columns" :data="libraryStore.libraries" :bordered="false" size="small" />
    </NCard>

    <NModal v-model:show="showCreate" title="创建磁带库">
      <NCard style="width: 420px" title="新建磁带库" :bordered="false">
        <NSpace vertical>
          <span>名称</span>
          <NInput v-model:value="newName" placeholder="磁带库名称" />
          <span>驱动器数</span>
          <NInputNumber v-model:value="newDrives" :min="1" :max="8" />
          <span>数据槽位数</span>
          <NInputNumber v-model:value="newSlots" :min="1" :max="256" />
          <NText depth="3">SCSI LUN 总数: {{ newDrives + 1 }} (1 changer + {{ newDrives }} drives)</NText>
        </NSpace>
        <NSpace justify="end" style="margin-top: 16px">
          <NButton @click="showCreate = false">取消</NButton>
          <NButton type="primary" @click="handleCreate">创建</NButton>
        </NSpace>
      </NCard>
    </NModal>
  </div>
</template>
