/** 通用 API 响应类型 */

export interface LibraryRow {
  id: number;
  name: string;
  created_at: string;
  is_offline_storage: boolean;
}

export interface LibrariesResponse {
  libraries: LibraryRow[];
  db_path: string;
  online_count: number;
  vtl_scsi_lines: number;
  hint: string;
  product_limits: {
    max_online_libraries: number;
    max_drives_per_library: number;
    max_data_slots_per_library: number;
  };
}

export interface TapeRow {
  name: string;
  barcode: string;
  capacity_bytes: number;
  used_bytes: number;
  slot: number | null;
  shelf_name?: string | null;
  in_drive: boolean;
}

export interface TapesResponse {
  library: string;
  tapes: TapeRow[];
  total: number;
  offset: number;
  limit: number;
  truncated: boolean;
}

export interface LibraryStatus {
  library: string;
  tape_count: number;
  loaded_in_drives: number;
  drives: number;
  data_slots: number;
}

export interface LibrariesStatusResponse {
  libraries: LibraryStatus[];
}

export interface ShelfRow {
  id: number;
  name: string;
  is_default_unused: boolean;
}

export interface ShelvesResponse {
  library: string;
  shelves: ShelfRow[];
}

export interface EmptySlotsResponse {
  library: string;
  empty_slots: number[];
  empty_slot_count: number;
}

export interface LibraryDetailResponse {
  library: {
    id: number;
    name: string;
    created_at: string;
    is_offline_storage: boolean;
    tape_count: number;
    loaded_in_drives: number;
    drive_count: number;
    data_slots: number;
    mail_slots: number;
    max_drives: number;
    slots: number;
    can_delete_online: boolean;
    inventory_source: string;
    inventory_truncated?: boolean;
  };
  drives: { drive_id: number; tape_name: string | null; tape_barcode: string | null }[];
  tapes: TapeRow[];
  changer: {
    source: string;
    data_slots: { label: string; tape_name: string | null; barcode: string | null }[];
    drives: { label: string; tape_name: string | null; barcode: string | null }[];
    mailslots: { label: string; tape_name: string | null; barcode: string | null }[];
  };
}

export interface DensityLimit {
  code: number;
  label: string;
  min_bytes: number;
  max_bytes: number;
  min_human: string;
  max_human: string;
}

export interface DensityLimitsResponse {
  density_limits: DensityLimit[];
}

export interface DriftItem {
  tape: string;
  db: string | null;
  kernel: string | null;
}

export interface ReconcileResponse {
  ok: boolean;
  drift_count: number;
  fixes_applied: number;
  pull_updates: number;
  inventory_truncated: boolean;
  drifts: DriftItem[];
}

export interface AutoAlignResponse {
  ok: boolean;
  evacuated: number;
  fixes_applied: number;
  pull_updates: number;
  drifts_remaining: number;
}

export interface PatrolResponse {
  exit_code: number;
  stdout: string;
  stderr: string;
  ok: string[];
  warn: string[];
  crit: string[];
}

export interface LoginResponse {
  ok: boolean;
  must_change_password: boolean;
}

export interface SessionInfo {
  token_prefix: string;
  username: string;
  created_secs_ago: number;
  is_current: boolean;
}

export interface SessionsResponse {
  sessions: SessionInfo[];
  count: number;
}

export interface OkResponse {
  ok: boolean;
  warning?: string;
}

export interface FabricResponse {
  transport: string;
  iscsi_iqn: string | null;
  iscsi_portals: string | null;
  fc_wwpn: string | null;
  kernel_reload_on_db_change: boolean;
  kernel_geom_prefer_ioctl: boolean;
  vtl_reload_scan_delay_ms: number | null;
  log_max_bytes: number;
  iscsi_exports_in_db: unknown[];
  patrol_hint: string;
  product_limits: unknown;
}

export interface SetupStatusResponse {
  setup_required: boolean;
  defaults: {
    db_path: string;
    tape_dir: string;
    log_dir: string;
    kernel_vtl_reload_script: string;
    vtl_ko: string;
    vtl_reload_scan_delay_ms: number;
  };
}

// ── iSCSI / Transport ──

export interface IscsiConfigResponse {
  tape_dir: string;
  transport: string;
  iscsi_iqn: string | null;
  iscsi_portals: string | null;
  portal_ip_suggested: string;
  portal_port_suggested: number;
  vtladm_iscsi_path: string;
  allow_iscsi_exec: boolean;
  non_unix_build: boolean;
  kernel_reload_on_db_change: boolean;
  kernel_geom_prefer_ioctl: boolean;
}

export interface IscsiExecResult {
  ok: boolean;
  stdout: string;
  stderr: string;
  dry_run?: boolean;
  iqn?: string;
  export_id?: string;
  saved_to_db?: boolean;
  save_blocked_reason?: string;
  hint?: string;
}

export interface IscsiExportDefaultsResponse {
  library: string;
  iqn: string;
  export_id: string;
  backend_ch: string;
  backend_drives: string[];
  portal_ip: string;
  portal_port: number;
  drive_count: number;
  default_lun_map: number[];
  changer_sg: string | null;
  drive_sg: string[] | null;
  has_saved_export: boolean;
  exported_at: string | null;
  saved_drive_mismatch: boolean;
  can_export: boolean;
  export_blocked_reason: unknown;
  product_limits: unknown;
}

export interface IscsiAllowExecResponse {
  ok: boolean;
  allow_iscsi_exec: boolean;
}

export interface TransportScanResponse {
  library: string;
  transport?: string;
  note?: string;
  changer_sg: string | null;
  drive_sg: string[];
  drive_count?: number;
  picked_scsi_host?: number;
  devices?: { role: string; lun: number; sg: string; sch?: string; st?: string; index?: number }[];
  raw_tail?: string;
  product_limits?: unknown;
  error?: string;
}

// ── Monitor ──

export interface SystemSnapshot {
  cpu: { pct: number; num_cores: number };
  mem: { total_kb: number; used_kb: number; pct: number };
  disks: { name: string; read_bytes: number; write_bytes: number }[];
}

export interface CapacityPoint {
  ts: string;
  library: string;
  total_bytes: number;
  used_bytes: number;
  tape_count: number;
}

export interface CapacityTrendResponse {
  points: CapacityPoint[];
}

export interface EventEntry {
  id: number;
  ts: string;
  category: string;
  action: string;
  detail: string;
}

export interface EventsResponse {
  events: EventEntry[];
}
