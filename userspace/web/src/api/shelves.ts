import { api } from './client';
import type { ShelvesResponse, OkResponse } from './types';

export function fetchShelves(library: string) {
  return api.get<ShelvesResponse>(`/api/shelves?library=${encodeURIComponent(library)}`);
}

export function fetchOfflineShelves() {
  return api.get<ShelvesResponse>('/api/offline-shelves');
}

export function createShelf(library: string, name: string) {
  return api.post<OkResponse>('/api/manage/shelf/create', { library, name });
}

export function deleteShelf(library: string, name: string) {
  return api.post<OkResponse>('/api/manage/shelf/delete', { library, name });
}

export function createOfflineShelf(name: string) {
  return api.post<OkResponse>('/api/manage/shelf/create-offline', { name });
}
