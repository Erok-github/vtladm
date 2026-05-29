<script setup lang="ts">
import { NMessageProvider, NNotificationProvider, NDialogProvider, NConfigProvider, zhCN, darkTheme } from 'naive-ui';
import { useRouter } from 'vue-router';
import { onMounted, onUnmounted } from 'vue';

const router = useRouter();

function sendLogoutBeacon() {
  navigator.sendBeacon('/api/logout');
}

onMounted(() => {
  window.addEventListener('beforeunload', sendLogoutBeacon);
});

onUnmounted(() => {
  window.removeEventListener('beforeunload', sendLogoutBeacon);
});
</script>

<template>
  <NConfigProvider :locale="zhCN">
    <NDialogProvider>
      <NNotificationProvider>
        <NMessageProvider>
          <router-view />
        </NMessageProvider>
      </NNotificationProvider>
    </NDialogProvider>
  </NConfigProvider>
</template>
