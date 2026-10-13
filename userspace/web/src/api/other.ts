import { api } from './client';
import type {
  EmptySlotsResponse,
  PatrolResponse,
  FabricResponse,
  IscsiConfigResponse,
  IscsiExecResult,
  IscsiExportDefaultsResponse,
  IscsiAllowExecResponse,
  TransportScanResponse,
  SystemSnapshot,
  CapacityTrendResponse,
  EventsResponse,
} from './types';

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

// ── iSCSI ──

export function fetchIscsiConfig() {
  return api.get<IscsiConfigResponse>('/api/manage/iscsi/config');
}

export function checkIscsi(sudo = false) {
  return api.post<IscsiExecResult>('/api/manage/iscsi/check', { sudo });
}

export function setIscsiAllowExec(allow: boolean) {
  return api.post<IscsiAllowExecResponse>('/api/manage/iscsi/allow-exec', { allow });
}

export function fetchIscsiExportDefaults(library: string, regenerate = false) {
  const qs = `library=${encodeURIComponent(library)}${regenerate ? '&regenerate=1' : ''}`;
  return api.get<IscsiExportDefaultsResponse>(`/api/manage/iscsi/library-export-defaults?${qs}`);
}

export function libraryIscsiExport(params: {
  library: string;
  backend?: string;
  iqn?: string;
  export_id?: string;
  changer_sg: string;
  drive_sg: string[];
  lun_map?: number[];
  portal_ip: string;
  portal_port: number;
  dry_run?: boolean;
  sudo?: boolean;
}) {
  return api.post<IscsiExecResult>('/api/manage/iscsi/library-export', params);
}

export function libraryIscsiUnexport(params: {
  library: string;
  backend?: string;
  iqn?: string;
  export_id?: string;
  lun_map?: number[];
  dry_run?: boolean;
  sudo?: boolean;
}) {
  return api.post<IscsiExecResult>('/api/manage/iscsi/library-unexport', params);
}

// ── SCSI Scan ──

export function scanTransportSg(library: string) {
  return api.get<TransportScanResponse>(
    `/api/manage/transport/scan-sg?library=${encodeURIComponent(library)}`,
  );
}

// ── Monitor ──

export function fetchSystemSnapshot() {
  return api.get<SystemSnapshot>('/api/monitor/system');
}

export function fetchCapacityTrend(library?: string, limit = 50) {
  const qs = library ? `?library=${encodeURIComponent(library)}&limit=${limit}` : `?limit=${limit}`;
  return api.get<CapacityTrendResponse>(`/api/monitor/capacity-trend${qs}`);
}

export function fetchEvents(limit = 50, category?: string) {
  const params = new URLSearchParams();
  params.set('limit', String(limit));
  if (category) params.set('category', category);
  return api.get<EventsResponse>(`/api/monitor/events?${params.toString()}`);
}
