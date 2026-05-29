<script setup lang="ts">
import { NSelect, NButton, NTag } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { useAuthStore } from '@/stores/auth';
import { computed } from 'vue';

const libraryStore = useLibraryStore();
const authStore = useAuthStore();

const libraryOptions = computed(() =>
  libraryStore.onlineLibraries.map((l) => ({
    label: `${l.name} (${l.id})`,
    value: l.name,
  })),
);

function handleLibraryChange(lib: string) {
  libraryStore.setCurrent(lib);
  // Reload current page data
  window.location.reload();
}
</script>

<template>
  <div class="top-bar">
    <div class="top-left">
      <span class="top-label">当前库：</span>
      <NSelect
        v-if="libraryStore.onlineLibraries.length > 0"
        :value="libraryStore.currentLibrary"
        :options="libraryOptions"
        size="small"
        style="width: 200px"
        @update:value="handleLibraryChange"
      />
      <span v-else class="top-no-lib">——</span>
    </div>
    <div class="top-right">
      <NTag type="info" size="small">已登录</NTag>
      <NButton text size="small" @click="authStore.logout()">登出</NButton>
    </div>
  </div>
</template>

<style scoped>
.top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 48px;
  padding: 0 24px;
  background: #fff;
  border-bottom: 1px solid #e8eaed;
}

.top-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.top-label {
  font-size: 13px;
  color: #666;
  white-space: nowrap;
}

.top-no-lib {
  color: #ccc;
}

.top-right {
  display: flex;
  align-items: center;
  gap: 12px;
}
</style>
