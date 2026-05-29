import { api } from './client';
import type { EmptySlotsResponse, PatrolResponse, FabricResponse } from './types';

export function fetchEmptySlots(library: string) {
  return api.get<EmptySlotsResponse>(`/api/empty-slots?library=${encodeURIComponent(library)}`);
}

export function fetchPatrol() {
  return api.get<PatrolResponse>('/api/patrol');
}

export function fetchFabric() {
  return api.get<FabricResponse>('/api/fabric');
}

export function fetchStatus(library: string) {
  return api.get<{ library: string; tape_count: number; loaded_in_drives: number; drives: number; data_slots: number }>(
    `/api/status?library=${encodeURIComponent(library)}`,
  );
}
