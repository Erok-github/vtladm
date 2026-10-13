<script setup lang="ts">
import { ref, onMounted, computed, h } from 'vue';
import { useRouter } from 'vue-router';
import {
  NGrid, NGi, NCard, NStatistic, NButton, NTag, NSpace,
  NSpin, NEmpty, NDataTable, NProgress,
} from 'naive-ui';
import type { DataTableColumns } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchLibrariesStatus } from '@/api/libraries';
import {
  fetchPatrol, fetchFabric, fetchSystemSnapshot,
  fetchCapacityTrend, fetchEvents,
} from '@/api/other';
import type {
  LibraryStatus, PatrolResponse,
  SystemSnapshot, CapacityPoint, EventEntry,
} from '@/api/types';

const router = useRouter();
const libraryStore = useLibraryStore();
const loading = ref(true);
const patrol = ref<PatrolResponse | null>(null);
const fabricTransport = ref('');
const statuses = ref<LibraryStatus[]>([]);
const systemSnapshot = ref<SystemSnapshot | null>(null);
const capacityPoints = ref<CapacityPoint[]>([]);
const events = ref<EventEntry[]>([]);

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

function fmtBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024 * 1024)).toFixed(1) + ' TB';
  if (b >= 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
  if (b >= 1024 * 1024) return (b / (1024 * 1024)).toFixed(0) + ' MB';
  return b + ' B';
}

const libColumns: DataTableColumns<LibSummary> = [
  { title: '库名', key: 'name', width: 130 },
  { title: '驱动器', key: 'drives', width: 70, render: (r) => `${r.drives}` },
  { title: '插槽', key: 'slots', width: 60, render: (r) => `${r.slots}` },
  { title: '磁带', key: 'tapes', width: 60, render: (r) => `${r.tapes}` },
  {
    title: '占用', key: 'usage', width: 80,
    render(r) { return r.slots > 0 ? `${Math.round((r.tapes / r.slots) * 100)}%` : '0%'; },
  },
  { title: '传输', key: 'transport', width: 70 },
  {
    title: '操作', key: 'actions', width: 80,
    render(row) {
      const lib = libraryStore.libraries.find((l) => l.name === row.name);
      if (!lib) return null;
      return h(NButton, { size: 'tiny', onClick: () => router.push(`/libraries/${lib.id}`) }, { default: () => '详情' });
    },
  },
];

// Capacity trend summary: pick latest point per library
const latestCapacity = computed(() => {
  const map = new Map<string, CapacityPoint>();
  for (const p of capacityPoints.value) {
    if (!map.has(p.library)) map.set(p.library, p);
  }
  return Array.from(map.values());
});

onMounted(async () => {
  await libraryStore.loadLibraries();

  try {
    const s = await fetchLibrariesStatus();
    statuses.value = s.libraries;
  } catch { console.error('加载库状态失败'); }

  try { patrol.value = await fetchPatrol(); } catch { /* optional */ }
  try {
    const f = await fetchFabric();
    fabricTransport.value = f.transport || '';
  } catch { /* optional */ }

  // Load monitor data
  try { systemSnapshot.value = await fetchSystemSnapshot(); } catch { /* optional */ }
  try {
    const trend = await fetchCapacityTrend(undefined, 200);
    capacityPoints.value = trend.points;
  } catch { /* optional */ }
  try {
    const ev = await fetchEvents(20);
    events.value = ev.events;
  } catch { /* optional */ }

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
        <NGrid responsive="screen" cols="1 s:2 m:3 l:4" :x-gap="12" :y-gap="12" style="margin-bottom: 16px">
          <NGi><NCard size="small" :bordered="true"><NStatistic label="在线库" :value="totalOnline" /></NCard></NGi>
          <NGi><NCard size="small" :bordered="true"><NStatistic label="驱动器总计" :value="totalDrives" /></NCard></NGi>
          <NGi><NCard size="small" :bordered="true"><NStatistic label="插槽总计" :value="totalSlots" /></NCard></NGi>
          <NGi><NCard size="small" :bordered="true"><NStatistic label="磁带总计" :value="totalTapes" /></NCard></NGi>
        </NGrid>

        <div v-if="fabricTransport" style="font-size:13px;color:#666;margin-bottom:8px">
          传输模式: {{ fabricTransport }}
        </div>

        <NDataTable
          v-if="libSummaries.length > 0"
          :columns="libColumns" :data="libSummaries" :bordered="false" size="small"
        />
        <NEmpty v-else description="暂无磁带库">
          <NButton size="small" style="margin-top:8px" @click="router.push('/libraries')">去建库</NButton>
        </NEmpty>
      </NCard>

      <!-- 性能监控 -->
      <NCard title="性能监控" size="small" style="margin-bottom: 16px">
        <NGrid responsive="screen" cols="1 s:2 m:3" :x-gap="12">
          <NGi>
            <NCard size="small" v-if="systemSnapshot" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:24px;font-weight:700">{{ systemSnapshot.cpu.pct.toFixed(1) }}%</div>
              <div style="font-size:12px;color:#999;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">CPU 使用率 ({{ systemSnapshot.cpu.num_cores }} 核)</div>
              <NProgress type="line" :percentage="Math.round(Math.min(systemSnapshot.cpu.pct, 100))" :height="6" style="margin-top:8px" />
            </NCard>
            <NCard v-else size="small" style="text-align:center;padding:24px;color:#999">驱动器吞吐量<br><small>暂无数据</small></NCard>
          </NGi>
          <NGi>
            <NCard size="small" v-if="systemSnapshot" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:24px;font-weight:700">{{ (systemSnapshot.mem.pct).toFixed(1) }}%</div>
              <div style="font-size:11px;color:#999;margin-top:2px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">已用 {{ (systemSnapshot.mem.used_kb / 1024 / 1024).toFixed(1) }} GB</div>
              <div style="font-size:11px;color:#999;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">共 {{ (systemSnapshot.mem.total_kb / 1024 / 1024).toFixed(1) }} GB</div>
              <NProgress type="line" :percentage="Math.round(systemSnapshot.mem.pct)" :height="6" style="margin-top:8px" />
            </NCard>
            <NCard v-else size="small" style="text-align:center;padding:24px;color:#999">聚合性能<br><small>暂无数据</small></NCard>
          </NGi>
          <NGi>
            <NCard size="small" v-if="systemSnapshot && systemSnapshot.disks.length > 0" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:13px;font-weight:600;margin-bottom:4px">磁盘 IO</div>
              <div v-for="d in systemSnapshot.disks.slice(0, 3)" :key="d.name" style="font-size:11px;color:#666;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                {{ d.name }}: 读 {{ fmtBytes(d.read_bytes) }} / 写 {{ fmtBytes(d.write_bytes) }}
              </div>
            </NCard>
            <NCard v-else size="small" style="text-align:center;padding:24px;color:#999">系统性能<br><small>暂无数据</small></NCard>
          </NGi>
        </NGrid>
      </NCard>

      <!-- 资源利用率 -->
      <NCard title="资源利用率" size="small" style="margin-bottom: 16px">
        <NGrid responsive="screen" cols="1 s:2 m:3" :x-gap="12">
          <NGi>
            <NCard size="small" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:13px;font-weight:600">驱动器使用率</div>
              <div v-if="totalDrives > 0" style="font-size:12px;color:#666;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                {{ statuses.reduce((s, l) => s + l.loaded_in_drives, 0) }} / {{ totalDrives }} 已装载
                <NProgress type="line" :percentage="totalDrives > 0 ? Math.round(statuses.reduce((s,l) => s + l.loaded_in_drives, 0) / totalDrives * 100) : 0" :height="6" style="margin-top:4px" />
              </div>
              <div v-else style="color:#999;font-size:12px">暂无数据</div>
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small" v-if="latestCapacity.length > 0" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:13px;font-weight:600">存储容量趋势</div>
              <div v-for="cp in latestCapacity" :key="cp.library" style="font-size:11px;color:#666;margin-top:4px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                {{ cp.library }}: {{ fmtBytes(cp.used_bytes) }} / {{ fmtBytes(cp.total_bytes) }}
                <NProgress type="line" :percentage="cp.total_bytes > 0 ? Math.round(cp.used_bytes / cp.total_bytes * 100) : 0" :height="4" style="margin-top:2px" />
              </div>
            </NCard>
            <NCard v-else size="small" style="text-align:center;padding:24px;color:#999">存储容量趋势<br><small>暂无数据 — 容量快照由定时巡检测试写入</small></NCard>
          </NGi>
          <NGi>
            <NCard size="small" v-if="systemSnapshot" style="text-align:center;padding:16px;overflow:hidden">
              <div style="font-size:13px;font-weight:600;margin-bottom:6px">系统资源</div>
              <div style="font-size:11px;color:#666;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">CPU: {{ systemSnapshot.cpu.num_cores }} 核</div>
              <div style="font-size:11px;color:#666;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">内存: {{ (systemSnapshot.mem.total_kb / 1024 / 1024).toFixed(1) }} GB</div>
              <div style="font-size:11px;color:#666;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">磁盘: {{ systemSnapshot.disks.length > 0 ? systemSnapshot.disks[0].name : '—' }}</div>
              <NProgress type="line" :percentage="Math.round(systemSnapshot.mem.pct)" :height="4" style="margin-top:8px" :color="systemSnapshot.mem.pct > 90 ? '#d03050' : '#18a058'" />
            </NCard>
            <NCard v-else size="small" style="text-align:center;padding:24px;color:#999">系统资源<br><small>暂无数据</small></NCard>
          </NGi>
        </NGrid>
      </NCard>

      <!-- 状态与事件 -->
      <NCard title="状态与事件" size="small">
        <NGrid responsive="screen" cols="1 m:2" :x-gap="12">
          <NGi>
            <div style="font-size:14px;font-weight:600;margin-bottom:8px">巡检测试</div>
            <template v-if="patrol">
              <NSpace>
                <template v-for="(item, idx) in patrol.ok" :key="'ok-'+idx"><NTag type="success" size="small">{{ item }}</NTag></template>
                <template v-for="(item, idx) in patrol.warn" :key="'warn-'+idx"><NTag type="warning" size="small">{{ item }}</NTag></template>
                <template v-for="(item, idx) in patrol.crit" :key="'crit-'+idx"><NTag type="error" size="small">{{ item }}</NTag></template>
              </NSpace>
              <div v-if="!patrol.ok.length && !patrol.warn.length && !patrol.crit.length" style="color:#999;font-size:13px;margin-top:4px">无异常项</div>
            </template>
            <div v-else style="color:#999;font-size:13px">巡检测试 — 暂无数据</div>
          </NGi>
          <NGi>
            <div style="font-size:14px;font-weight:600;margin-bottom:8px">操作日志</div>
            <template v-if="events.length > 0">
              <div v-for="ev in events.slice(0, 8)" :key="ev.id" style="font-size:11px;color:#666;padding:2px 0;border-bottom:1px solid #f5f5f5;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                <span style="color:#999">{{ ev.ts }}</span>
                <NTag size="tiny" style="margin:0 4px">{{ ev.category }}</NTag>
                {{ ev.action }} — {{ ev.detail }}
              </div>
            </template>
            <div v-else style="color:#999;font-size:13px">操作日志 — 暂无数据</div>
          </NGi>
        </NGrid>
      </NCard>
    </NSpin>
  </div>
</template>
