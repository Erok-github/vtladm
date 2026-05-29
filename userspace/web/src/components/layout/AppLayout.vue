<script setup lang="ts">
import { onMounted } from 'vue';
import { useLibraryStore } from '@/stores/library';
import SideNav from './SideNav.vue';
import TopBar from './TopBar.vue';

const libraryStore = useLibraryStore();

onMounted(async () => {
  await libraryStore.loadLibraries();
});
</script>

<template>
  <div class="app-shell">
    <aside class="app-sidebar">
      <SideNav />
    </aside>
    <div class="app-body">
      <TopBar />
      <main class="app-main">
        <router-view />
      </main>
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  min-height: 100vh;
  background: #f5f7fa;
}

.app-sidebar {
  width: 240px;
  flex-shrink: 0;
  background: #fff;
  border-right: 1px solid #e8eaed;
  display: flex;
  flex-direction: column;
}

.app-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.app-main {
  flex: 1;
  padding: 16px 24px;
  overflow-y: auto;
}

@media (max-width: 1024px) {
  .app-sidebar {
    width: 64px;
  }
}
</style>
