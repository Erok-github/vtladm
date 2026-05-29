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
    data_slots: { slot_id: number; tape_name: string | null; barcode: string | null }[];
    drives: { drive_id: number; tape_name: string | null; barcode: string | null }[];
    mailslots: { slot_id: number; tape_name: string | null; barcode: string | null }[];
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
