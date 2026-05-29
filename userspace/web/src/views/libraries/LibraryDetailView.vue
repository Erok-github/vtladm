<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { useRoute } from 'vue-router';
import { NCard, NGrid, NGi, NStatistic, NTag, NSpin, NProgress } from 'naive-ui';
import { useLibraryStore } from '@/stores/library';
import { fetchLibraryDetail } from '@/api/libraries';
import type { LibraryDetailResponse } from '@/api/types';

const route = useRoute();
const libraryStore = useLibraryStore();
const detail = ref<LibraryDetailResponse | null>(null);
const loading = ref(true);

function fmtBytes(b: number): string {
  if (b >= 1024 * 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024 * 1024)).toFixed(1) + ' TB';
  if (b >= 1024 * 1024 * 1024) return (b / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
  return b + ' B';
}

const id = computed(() => route.params.id as string);

onMounted(async () => {
  // Resolve library name from ID
  await libraryStore.loadLibraries();
  const lib = libraryStore.libraries.find((l) => String(l.id) === id.value);
  if (!lib) return;
  try {
    detail.value = await fetchLibraryDetail(lib.name);
  } catch {
    // ignore
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div>
    <h2 style="margin: 0 0 16px; font-size: 20px;">库详情</h2>

    <NSpin :show="loading">
      <template v-if="detail">
        <NGrid :cols="3" :x-gap="12" :y-gap="12" style="margin-bottom: 20px">
          <NGi>
            <NCard size="small">
              <NStatistic label="驱动器" :value="detail.drives.length" />
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small">
              <NStatistic label="数据槽位" :value="detail.changer.data_slots.length" />
            </NCard>
          </NGi>
          <NGi>
            <NCard size="small">
              <NStatistic label="磁带" :value="detail.tapes.length" />
            </NCard>
          </NGi>
        </NGrid>

        <NCard title="驱动器状态" size="small" style="margin-bottom: 16px">
          <div v-for="d in detail.drives" :key="d.drive_id" style="margin-bottom: 8px">
            <NTag :type="d.tape_name ? 'success' : 'default'" size="small">
              Drive {{ d.drive_id }}: {{ d.tape_name || '(空)' }}
            </NTag>
            <span v-if="d.tape_barcode" style="font-size:12px;color:#999;margin-left:8px">{{ d.tape_barcode }}</span>
          </div>
        </NCard>

        <NCard title="槽位占用" size="small">
          <div style="display:flex;flex-wrap:wrap;gap:6px">
            <template v-for="s in detail.changer.data_slots" :key="s.slot_id">
              <NTag :type="s.tape_name ? 'info' : 'default'" size="small">
                {{ s.slot_id }}: {{ s.tape_name || '空' }}
              </NTag>
            </template>
          </div>
        </NCard>
      </template>
      <div v-else style="color:#999;text-align:center;padding:40px">库不存在或已删除</div>
    </NSpin>
  </div>
</template>
