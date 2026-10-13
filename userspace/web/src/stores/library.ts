import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { fetchLibraries, fetchLibrariesStatus } from '@/api/libraries';
import type { LibraryRow, LibraryStatus } from '@/api/types';

export const useLibraryStore = defineStore('library', () => {
  const libraries = ref<LibraryRow[]>([]);
  const statuses = ref<LibraryStatus[]>([]);
  const currentLibrary = ref<string | null>(null);
  const loading = ref(false);

  const onlineLibraries = computed(() =>
    libraries.value.filter((l) => !l.is_offline_storage),
  );

  function setCurrent(lib: string) {
    currentLibrary.value = lib;
  }

  async function loadLibraries() {
    loading.value = true;
    try {
      const data = await fetchLibraries();
      libraries.value = data.libraries;
      // Auto-select first online library if none selected
      const online = data.libraries.filter((l) => !l.is_offline_storage);
      if (!currentLibrary.value && online.length > 0) {
        currentLibrary.value = online[0].name;
      }
    } catch {
      // ignore
    } finally {
      loading.value = false;
    }
  }

  async function loadStatuses() {
    try {
      const data = await fetchLibrariesStatus();
      statuses.value = data.libraries;
    } catch {
      // ignore
    }
  }

  return {
    libraries,
    statuses,
    currentLibrary,
    loading,
    onlineLibraries,
    setCurrent,
    loadLibraries,
    loadStatuses,
  };
});
