import { api } from './client';
import type { ReconcileResponse, AutoAlignResponse, OkResponse } from './types';

export function robotSync(library: string) {
  return api.post<OkResponse & { tapes_updated: number }>('/api/manage/robot/sync', { library });
}

export function robotReconcile(library: string, pull?: boolean) {
  return api.post<ReconcileResponse>('/api/manage/robot/reconcile', {
    library,
    ...(pull !== undefined && { pull }),
  });
}

export function robotAutoAlign(library: string) {
  return api.post<AutoAlignResponse>('/api/manage/robot/auto-align', { library });
}

//export function changerLoad(library: string, slot: number, drive: number) {
//  return api.post<OkResponse>('/api/manage/tape/load', { library, slot, drive });
//}
//
//export function changerUnload(library: string, drive: number) {
//  return api.post<OkResponse & { slot: number }>('/api/manage/tape/unload', { library, drive });
//}
//
//export function changerEject(library: string, slot: number) {
//  return api.post<OkResponse & { mailslot: number }>('/api/manage/tape/eject', { library, slot });
//}
