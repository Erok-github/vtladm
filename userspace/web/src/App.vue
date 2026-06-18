<script setup lang="ts">
import { NMessageProvider, NNotificationProvider, NDialogProvider, NConfigProvider, zhCN, darkTheme } from 'naive-ui';
import { useRouter } from 'vue-router';

const router = useRouter();
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
