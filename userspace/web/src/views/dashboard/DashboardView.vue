<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { NGrid, NGi, NCard, NStatistic, NButton, NProgress, NTag, NSpace, NSpin } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchLibrariesStatus, fetchLibraryDetail } from '@/api/libraries';
import { fetchPatrol } from '@/api/other';
import type { LibraryStatus, LibraryDetailResponse } from '@/api/types';

const router = useRouter();
const libraryStore = useLibraryStore();
const loading = ref(true);
const statuses = ref<LibraryStatus[]>([]);
const details = ref<Map<string, LibraryDetailResponse>>(new Map());
const patrolResult = ref<{ ok: string[]; warn: string[]; crit: string[] } | null>(null);

// Helper: format bytes
function fmtBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024 * 1024)).toFixed(1) + ' TB';
  if (b >= 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
  if (b >= 1024 * 1024) return (b / (1024 * 1024)).toFixed(0) + ' MB';
  return b + ' B';
}

onMounted(async () => {
  await libraryStore.loadLibraries();
  await libraryStore.loadStatuses();
  statuses.value = libraryStore.statuses;

  // Load per-library details
  for (const lib of libraryStore.onlineLibraries) {
    try {
      const d = await fetchLibraryDetail(lib.name);
      details.value.set(lib.name, d);
    } catch {
      // ignore
    }
  }

  // Load patrol
  try {
    const p = await fetchPatrol();
    patrolResult.value = { ok: p.ok, warn: p.warn, crit: p.crit };
  } catch {
    // ignore
  }

  loading.value = false;
});

// Aggregate stats across all libraries
function totalTapeCount(): number {
  let n = 0;
  for (const d of details.value.values()) n += d.tapes.length;
  return n;
}

function totalDrives(): number {
  return statuses.value.reduce((s, l) => s + l.drives, 0);
}

function totalUsedBytes(): number {
  let n = 0;
  for (const d of details.value.values()) {
    for (const t of d.tapes) n += t.used_bytes;
  }
  return n;
}

function totalCapacityBytes(): number {
  let n = 0;
  for (const d of details.value.values()) {
    for (const t of d.tapes) n += t.capacity_bytes;
  }
  return n;
}

const quickActions = [
  { label: '建库', path: '/libraries' },
  { label: '建带', path: '/tapes' },
  { label: '磁带入槽', path: '/assign-slot' },
  { label: 'Inventory 对账', path: '/changer' },
  { label: '传输向导', path: '/transport' },
  { label: 'iSCSI 映射', path: '/transport' },
  { label: '货架管理', path: '/shelves' },
  { label: '账户安全', path: '/account' },
];
</script>

<template>
  <div>
    <h2 style="margin: 0 0 16px; font-size: 20px;">仪表盘</h2>

    <NSpin :show="loading">
      <!-- Stats cards -->
      <NGrid :cols="4" :x-gap="12" :y-gap="12" style="margin-bottom: 20px" responsive="screen">
        <NGi>
          <NCard size="small">
            <NStatistic label="磁带库" :value="libraryStore.onlineLibraries.length" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="磁带总数" :value="totalTapeCount()" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="已用容量" :value="fmtBytes(totalUsedBytes())" />
          </NCard>
        </NGi>
        <NGi>
          <NCard size="small">
            <NStatistic label="在线驱动器" :value="totalDrives()" />
          </NCard>
        </NGi>
      </NGrid>

      <!-- Library overview -->
      <NCard title="库概览" size="small" style="margin-bottom: 20px">
        <div v-if="libraryStore.onlineLibraries.length === 0" style="color: #999; padding: 20px; text-align: center">
          暂无在线库，请先<a href="/libraries" @click.prevent="router.push('/libraries')">创建磁带库</a>
        </div>
        <div v-for="lib in libraryStore.onlineLibraries" :key="lib.name" style="margin-bottom: 12px">
          <div style="display: flex; justify-content: space-between; margin-bottom: 4px">
            <span style="font-weight: 600">{{ lib.name }}</span>
            <span style="font-size: 13px; color: #666">
              {{ details.get(lib.name)?.drives.length ?? 0 }} 驱动器
              × {{ details.get(lib.name)?.changer?.data_slots?.length ?? 0 }} 槽位
              · 磁带 {{ details.get(lib.name)?.tapes?.length ?? 0 }}
            </span>
          </div>
          <NProgress
            type="line"
            :percentage="details.has(lib.name)
              ? Math.round(totalUsedBytes() / Math.max(totalCapacityBytes(), 1) * 100)
              : 0"
            :height="16"
            :border-radius="4"
          />
        </div>
      </NCard>

      <!-- Quick actions + Patrol -->
      <NGrid :cols="2" :x-gap="12" responsive="screen">
        <NGi>
          <NCard title="快速操作" size="small">
            <NSpace wrap>
              <NButton
                v-for="act in quickActions"
                :key="act.label"
                size="small"
                @click="router.push(act.path)"
              >
                {{ act.label }}
              </NButton>
            </NSpace>
          </NCard>
        </NGi>
        <NGi>
          <NCard title="巡检测试" size="small">
            <template v-if="patrolResult">
              <div v-if="patrolResult.ok.length" style="color: #18a058">
                ✓ {{ patrolResult.ok.map(s => s).join(', ') }}
              </div>
              <div v-if="patrolResult.warn.length" style="color: #f0a020">
                ⚠ {{ patrolResult.warn.map(s => s).join(', ') }}
              </div>
              <div v-if="patrolResult.crit.length" style="color: #d03050">
                ✗ {{ patrolResult.crit.map(s => s).join(', ') }}
              </div>
              <div v-if="!patrolResult.ok.length && !patrolResult.warn.length && !patrolResult.crit.length" style="color: #999">
                暂无巡检测试数据
              </div>
            </template>
            <template v-else>
              <span style="color: #999">——</span>
            </template>
          </NCard>
        </NGi>
      </NGrid>
    </NSpin>
  </div>
</template>
