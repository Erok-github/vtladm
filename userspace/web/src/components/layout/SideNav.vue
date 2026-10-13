<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';
import { NMenu } from 'naive-ui';
import type { MenuOption } from 'naive-ui';

const route = useRoute();

const menuOptions: MenuOption[] = [
  {
    label: '仪表盘',
    key: '/',
  },
  {
    label: '磁带库',
    key: 'group-lib',
    type: 'group',
    children: [
      { label: '库管理', key: '/libraries' },
    ],
  },
  {
    label: '账户安全',
    key: '/account',
  },
];

const activeKey = computed(() => {
  const p = route.path;
  if (p === '/') return '/';
  if (p === '/account') return '/account';
  if (p.startsWith('/libraries')) return '/libraries';
  return '/';
});
</script>

<template>
  <div class="side-nav">
    <div class="side-brand">
      <span class="brand-icon">📼</span>
      <span class="brand-text">VTL 控制台</span>
    </div>
    <div class="side-hint">虚拟磁带库 · 备份存储层</div>
    <NMenu
      :options="menuOptions"
      :value="activeKey"
      :indent="20"
      @update:value="(key: string) => $router.push(key)"
    />
    <div class="side-footer">
      <span style="font-size:12px;color:#999">v1.0.0</span>
    </div>
  </div>
</template>

<style scoped>
.side-nav {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.side-brand {
  padding: 16px;
  font-size: 18px;
  font-weight: 700;
  color: #0d47a1;
  display: flex;
  align-items: center;
  gap: 8px;
  border-bottom: 1px solid #e8eaed;
}

.side-hint {
  padding: 8px 16px;
  font-size: 12px;
  color: #999;
  border-bottom: 1px solid #f0f0f0;
}

.side-footer {
  margin-top: auto;
  padding: 12px 16px;
  border-top: 1px solid #e8eaed;
}
</style>
