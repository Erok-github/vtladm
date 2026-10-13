<script setup lang="ts">
import { NMessageProvider, NNotificationProvider, NDialogProvider, NConfigProvider, zhCN } from 'naive-ui';
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
