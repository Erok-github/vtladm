<script setup lang="ts">
import { ref, onMounted, computed, h } from 'vue';
import { useRouter } from 'vue-router';
import { NGrid, NGi, NCard, NStatistic, NButton, NTag, NSpace, NSpin, NEmpty, NDataTable } from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchLibrariesStatus } from '@/api/libraries';
import { fetchPatrol, fetchFabric } from '@/api/other';
import type { LibraryStatus, PatrolResponse } from '@/api/types';

const router = useRouter();
const libraryStore = useLibraryStore();
const loading = ref(true);
const patrol = ref<PatrolResponse | null>(null);
const fabricTransport = ref('');
const statuses = ref<LibraryStatus[]>([]);

interface LibSummary {
  name: string;
  drives: number;
  slots: number;
  tapes: number;
  transport: string;
}

const libSummaries = ref<LibSummary[]>([]);

const totalOnline = computed(() => libraryStore.onlineLibraries.length);
const totalDrives = computed(() => statuses.value.reduce((s, l) => s + l.drives, 0));
const totalSlots = computed(() => statuses.value.reduce((s, l) => s + l.data_slots, 0));
const totalTapes = computed(() => statuses.value.reduce((s, l) => s + l.tape_count, 0));

const libColumns: DataTableColumns<LibSummary> = [
  { title: '库名', key: 'name', width: 130 },
  { title: '驱动器', key: 'drives', width: 70, render: (r) => `${r.drives}` },
  { title: '插槽', key: 'slots', width: 60, render: (r) => `${r.slots}` },
  { title: '磁带', key: 'tapes', width: 60, render: (r) => `${r.tapes}` },
  {
    title: '占用',
    key: 'usage',
    width: 80,
    render(r) {
      const pct = r.slots > 0 ? Math.round((r.tapes / r.slots) * 100) : 0;
      return `${pct}%`;
    },
  },
  { title: '传输', key: 'transport', width: 70 },
  {
    title: '操作',
    key: 'actions',
    width: 80,
    render(row) {
      const lib = libraryStore.libraries.find((l) => l.name === row.name);
      if (!lib) return null;
      return h(NButton, { size: 'tiny', onClick: () => router.push(`/libraries/${lib.id}`) }, { default: () => '详情' });
    },
  },
];

onMounted(async () => {
  await libraryStore.loadLibraries();

  // statuses — critical, must not be blocked by patrol/fabric
  try {
    const s = await fetchLibrariesStatus();
    statuses.value = s.libraries;
  } catch {
    console.error('加载库状态失败');
  }

  try {
    patrol.value = await fetchPatrol();
  } catch {
    // patrol is optional
  }

  try {
    const f = await fetchFabric();
    fabricTransport.value = f.transport || '';
  } catch {
    // fabric is optional
  }

  libSummaries.value = libraryStore.onlineLibraries.map((lib) => {
    const st = statuses.value.find((s) => s.library === lib.name);
    return {
      name: lib.name,
      drives: st?.drives ?? 0,
      slots: st?.data_slots ?? 0,
      tapes: st?.tape_count ?? 0,
      transport: fabricTransport.value || '——',
    };
  });
  loading.value = false;
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 16px; font-size: 20px;">仪表盘</h2>

    <NSpin :show="loading">
      <!-- 资产管理 -->
      <NCard title="资产管理" size="small" style="margin-bottom: 16px">
        <NGrid :cols="4" :x-gap="12" :y-gap="12" style="margin-bottom: 16px">
          <NGi>
            <NCard size="small" :bordered="true">
              <NStatistic label="在线库" :value="totalOnline" />
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" :bordered="true">
              <NStatistic label="驱动器总计" :value="totalDrives" />
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" :bordered="true">
              <NStatistic label="插槽总计" :value="totalSlots" />
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" :bordered="true">
              <NStatistic label="磁带总计" :value="totalTapes" />
            </NCard>
          </NGi>
        </NGrid>

        <div v-if="fabricTransport" style="font-size:13px;color:#666;margin-bottom:8px">
          传输模式: {{ fabricTransport }}
        </div>

        <NDataTable
          v-if="libSummaries.length > 0"
          :columns="libColumns"
          :data="libSummaries"
          :bordered="false"
          size="small"
        />
        <NEmpty v-else description="暂无磁带库">
          <NButton size="small" style="margin-top:8px" @click="router.push('/libraries')">去建库</NButton>
        </NEmpty>
      </NCard>

      <!-- 性能监控 -->
      <NCard title="性能监控" size="small" style="margin-bottom: 16px">
        <NGrid :cols="3" :x-gap="12">
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              驱动器吞吐量<br><small>暂无数据</small>
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              聚合性能<br><small>暂无数据</small>
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              系统性能<br><small>CPU/内存/IO — 暂无数据</small>
            </NCard>
          </NGi>
        </NGrid>
      </NCard>

      <!-- 资源利用率 -->
      <NCard title="资源利用率" size="small" style="margin-bottom: 16px">
        <NGrid :cols="3" :x-gap="12">
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              驱动器使用率<br><small>暂无数据</small>
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              系统资源<br><small>CPU/内存/文件系统 — 暂无数据</small>
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" style="text-align:center;padding:24px;color:#999">
              存储容量趋势<br><small>暂无数据</small>
            </NCard>
          </NGi>
        </NGrid>
      </NCard>

      <!-- 状态与事件 -->
      <NCard title="状态与事件" size="small">
        <template v-if="patrol">
          <div style="margin-bottom: 12px; font-size: 14px; font-weight: 600">巡检测试</div>
          <NSpace>
            <template v-for="(item, idx) in patrol.ok" :key="'ok-'+idx">
              <NTag type="success" size="small">{{ item }}</NTag>
            </template>
            <template v-for="(item, idx) in patrol.warn" :key="'warn-'+idx">
              <NTag type="warning" size="small">{{ item }}</NTag>
            </template>
            <template v-for="(item, idx) in patrol.crit" :key="'crit-'+idx">
              <NTag type="error" size="small">{{ item }}</NTag>
            </template>
          </NSpace>
          <div
            v-if="!patrol.ok.length && !patrol.warn.length && !patrol.crit.length"
            style="color:#999;font-size:13px;margin-top:4px"
          >
            无异常项
          </div>
        </template>
        <div v-else style="color:#999;font-size:13px">巡检测试 — 暂无数据</div>
        <div style="margin-top: 16px; font-size: 14px; font-weight: 600; color: #999">
          操作日志<br><small>暂无数据 — 需后端暴露监控端点后对接</small>
        </div>
      </NCard>
    </NSpin>
  </div>
</template>
