<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { NCard, NButton, NInput, NSpace, NTag, NSpin } from 'naive-ui';
import { fetchFabric } from '@/api/other';
import type { FabricResponse } from '@/api/types';

function openRawFabric() {
  window.open('/browse/fabric', '_blank');
}

const fabric = ref<FabricResponse | null>(null);
const loading = ref(true);

onMounted(async () => {
  try {
    fabric.value = await fetchFabric();
  } catch {
    // ignore
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 16px; font-size: 20px;">传输配置</h2>

    <NSpin :show="loading">
      <NCard v-if="fabric" title="当前传输配置" size="small" style="margin-bottom: 16px">
        <div><strong>传输模式:</strong> {{ fabric.transport }}</div>
        <div v-if="fabric.iscsi_iqn"><strong>iSCSI IQN:</strong> {{ fabric.iscsi_iqn }}</div>
        <div v-if="fabric.iscsi_portals"><strong>iSCSI Portal:</strong> {{ fabric.iscsi_portals }}</div>
        <div v-if="fabric.fc_wwpn"><strong>FC WWPN:</strong> {{ fabric.fc_wwpn }}</div>
        <div style="margin-top: 8px; font-size: 12px; color: #999">
          内核重载: {{ fabric.kernel_reload_on_db_change ? '自动' : '手动' }}
        </div>
      </NCard>
      <NCard v-else size="small" style="margin-bottom: 16px">
        <div style="color: #999; text-align: center; padding: 20px">无法获取传输配置</div>
      </NCard>

      <NCard title="SCSI 设备扫描" size="small">
        <p style="font-size: 13px; color: #666">
          SCSI 设备扫描由内核模块管理。确认 <code>vtl.ko</code> 已加载后，备份软件可通过 <code>lsscsi -g</code> 发现 VTL 设备。
        </p>
        <NButton size="small" @click="openRawFabric()">
          查看原始传输数据
        </NButton>
      </NCard>

      <NCard title="iSCSI 导出" size="small" style="margin-top: 16px">
        <p style="font-size: 13px; color: #666">
          iSCSI 导出需要在服务器上配置 LIO target。请使用 <code>vtladm transport guide</code> CLI 命令
          或在服务器终端执行相关脚本。
        </p>
      </NCard>
    </NSpin>
  </div>
</template>
