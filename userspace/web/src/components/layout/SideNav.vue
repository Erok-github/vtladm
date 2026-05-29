<script setup lang="ts">
import { useRoute } from 'vue-router';
import { NMenu } from 'naive-ui';
import { h, computed } from 'vue';
import type { MenuOption } from 'naive-ui';

const route = useRoute();

function resolveActive(): string {
  return route.path;
}

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
      { label: '库详情', key: '/libraries/' },
    ],
  },
  {
    label: '磁带与槽位',
    key: 'group-tape',
    type: 'group',
    children: [
      { label: '磁带管理', key: '/tapes' },
      { label: '磁带入槽', key: '/assign-slot' },
      { label: '对账面板', key: '/changer' },
    ],
  },
  {
    label: '货架',
    key: 'group-shelf',
    type: 'group',
    children: [
      { label: '货架管理', key: '/shelves' },
    ],
  },
  {
    label: '传输',
    key: '/transport',
  },
  {
    label: '账户安全',
    key: '/account',
  },
];

const activeKey = computed(() => {
  const p = route.path;
  // Match the longest prefix
  const candidates = ['/libraries', '/tapes', '/assign-slot', '/changer', '/shelves', '/transport', '/account', '/'];
  for (const c of candidates) {
    if (p === c) return c;
  }
  if (p.startsWith('/libraries/')) return '/libraries/';
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
      <a href="/login" @click.prevent="$router.push('/account')">账户与安全</a>
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
  font-size: 13px;
}

.side-footer a {
  color: #0d47a1;
  text-decoration: none;
}
</style>
