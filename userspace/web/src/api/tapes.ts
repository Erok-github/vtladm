import { api } from './client';
import type { TapesResponse, DensityLimitsResponse, OkResponse } from './types';

export function fetchTapes(library: string, offset = 0, limit = 5000) {
  return api.get<TapesResponse>(
    `/api/tapes?library=${encodeURIComponent(library)}&offset=${offset}&limit=${limit}`,
  );
}

export function fetchDensityLimits() {
  return api.get<DensityLimitsResponse>('/api/manage/tape/density-limits');
}

export function createTape(library: string, name: string, size: string, shelf?: string, density?: string) {
  return api.post<OkResponse>('/api/manage/tape/create', {
    library,
    name,
    size,
    ...(shelf && { shelf }),
    ...(density && { density }),
  });
}

export function deleteTape(library: string, name: string) {
  return api.post<OkResponse & { warning?: string }>('/api/manage/tape/delete', {
    library,
    name,
  });
}

export function initTape(library: string, name: string) {
  return api.post<OkResponse>('/api/manage/tape/init', { library, name });
}

export function createTapeBatch(library: string, items: { name: string; size: string; density?: string }[], shelf?: string) {
  return api.post<OkResponse>('/api/manage/tape/create-batch', {
    library,
    shelf,
    items,
  });
}

export function createTapeAutoBatch(library: string, count: number, size: string, shelf?: string, density?: string) {
  return api.post<OkResponse & { names: string[]; count: number }>(
    '/api/manage/tape/create-auto-batch',
    { library, count, size, ...(shelf && { shelf }), ...(density && { density }) },
  );
}

export function assignSlot(library: string, tape: string, slot: number, fromOffline?: boolean) {
  return api.post<OkResponse>('/api/manage/tape/assign-slot', {
    library,
    tape,
    slot,
    ...(fromOffline !== undefined && { from_offline: fromOffline }),
  });
}

export function assignSlotBatch(library: string, pairs: { tape: string; slot: number; from_offline?: boolean }[]) {
  return api.post<OkResponse>('/api/manage/tape/assign-slot-batch', { library, pairs });
}

export function shelfPlace(library: string, tape: string, shelf?: string) {
  return api.post<OkResponse>('/api/manage/tape/shelf-place', {
    library,
    tape,
    ...(shelf && { shelf }),
  });
}

export function shelfPlaceBatch(library: string, tapes: string[], shelf: string) {
  return api.post<OkResponse>('/api/manage/tape/shelf-place-batch', {
    library,
    tapes,
    shelf,
  });
}

export function migrateShelvesBatch(library: string, fromShelf: string, toShelf: string, tapes: string[]) {
  return api.post<OkResponse>('/api/manage/tape/migrate-shelves-batch', {
    library,
    from_shelf: fromShelf,
    to_shelf: toShelf,
    tapes,
  });
}
