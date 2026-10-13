import { api } from './client';
import type {
  LibrariesResponse,
  LibraryDetailResponse,
  LibrariesStatusResponse,
  OkResponse,
} from './types';

export function fetchLibraries() {
  return api.get<LibrariesResponse>('/api/libraries');
}

export function fetchLibrariesStatus() {
  return api.get<LibrariesStatusResponse>('/api/libraries-status');
}

export function fetchLibraryDetail(library: string) {
  return api.get<LibraryDetailResponse>(`/api/library/detail?library=${encodeURIComponent(library)}`);
}

export function createLibrary(name: string, drives: number, slots: number) {
  return api.post<OkResponse & { kernel_geom: string; kernel_geom_detail: string; scsi_rescan: string }>(
    '/api/manage/library/create',
    { name, drives, slots },
  );
}

export function deleteLibrary(name: string) {
  return api.post<OkResponse & { file_warnings: string | null }>(
    '/api/manage/library/delete',
    { name },
  );
}
