use rusqlite::{params, Connection};

/// `cargo test` 用库名：可写入 DB，但**不出现在 Web 列表**且**不导出到 SCSI**（`lsscsi -g`）。
#[cfg(test)]
pub(crate) const LEGACY_DEFAULT_LIBRARY_NAME: &str = "default";
/// Production builds must keep legacy `default` libraries visible for upgraded installs.
#[cfg(not(test))]
pub(crate) const LEGACY_DEFAULT_LIBRARY_NAME: &str = "__vtladm_cargo_test_default__";
pub(crate) const DEFAULT_UNUSED_SHELF_NAME: &str = "unused";

/// 系统保留库：模拟离库后的磁带保管区（仅有货架，无机械手槽位）。
pub(crate) const OFFLINE_LIBRARY_NAME: &str = "__offline__";

/// 仅 `cargo test` 写入 DB 的库名：Web/API 列表与 `lsscsi` 展示均隐藏。
pub(crate) fn is_test_only_library_name(name: &str) -> bool {
    name == LEGACY_DEFAULT_LIBRARY_NAME
}

/// 参与内核 SCSI 导出的在线库（与 [`build_vtl_instances_kernel_spec`] 一致，不含 `__offline__` / `default`）。
pub(crate) fn is_kernel_exported_library_name(name: &str) -> bool {
    !is_test_only_library_name(name) && name != OFFLINE_LIBRARY_NAME
}

/// 在线带库个数（不含 `__offline__` / 测试库 `default`），用于删库判断与建库上限。
pub(crate) fn count_exported_online_libraries(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM vtl_libraries WHERE name NOT IN (?1, ?2)",
        params![OFFLINE_LIBRARY_NAME, LEGACY_DEFAULT_LIBRARY_NAME],
        |r| r.get(0),
    )
}

/// 与内核 `vtl.h` 中 `VTL_MAX_SCSI_INSTANCES` 一致：在线带库（`vtl_instances` 段数，不含 `__offline__`）的**个数上限**（可少于 8）。
pub(crate) const VTL_KERNEL_MAX_ONLINE_LIBRARIES: usize = 8;
/// 与内核 `VTL_MAX_DRIVES` 一致（每库磁带机个数的**上限**，方案 B：8）。
pub(crate) const VTL_KERNEL_MAX_DRIVES_PER_LIB: i32 = 8;
/// 与内核 `VTL_MAX_SLOTS` 一致（每库**数据**槽位个数的**上限**，不含固定 I/E mail 槽）。
pub(crate) const VTL_KERNEL_MAX_DATA_SLOTS_PER_LIB: i32 = 256;

/* T10 SCSI density codes (SSC-5) */
pub(crate) const DENSITY_DEFAULT: u8 = 0x40;
pub(crate) const DENSITY_LTO5: u8    = 0x4A;
pub(crate) const DENSITY_LTO6: u8    = 0x4C;
pub(crate) const DENSITY_LTO7: u8    = 0x4E;
pub(crate) const DENSITY_LTO8: u8    = 0x50;
pub(crate) const DENSITY_LTO9: u8    = 0x52;
pub(crate) const DENSITY_LTO10: u8   = 0x58;

/// Parse density string ("LTO-8", "lto-8", "0x50", "0x4C") to u8 code
pub(crate) fn parse_density(s: &str) -> Option<u8> {
    let s = s.trim();
    if s.is_empty() {
        return Some(DENSITY_DEFAULT);
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u8::from_str_radix(hex, 16).ok();
    }
    match s.to_uppercase().as_str() {
        "LTO-5" | "LTO5" => Some(DENSITY_LTO5),
        "LTO-6" | "LTO6" => Some(DENSITY_LTO6),
        "LTO-7" | "LTO7" => Some(DENSITY_LTO7),
        "LTO-8" | "LTO8" => Some(DENSITY_LTO8),
        "LTO-9" | "LTO9" => Some(DENSITY_LTO9),
        "LTO-10" | "LTO10" => Some(DENSITY_LTO10),
        "DEFAULT" | "LTO" | "ULTRIUM" => Some(DENSITY_DEFAULT),
        _ => None,
    }
}

/// Map density code to human-readable label
pub(crate) fn density_label(code: u8) -> &'static str {
    match code {
        DENSITY_DEFAULT => "Default LTO",
        DENSITY_LTO5 => "LTO-5",
        DENSITY_LTO6 => "LTO-6",
        DENSITY_LTO7 => "LTO-7",
        DENSITY_LTO8 => "LTO-8",
        DENSITY_LTO9 => "LTO-9",
        DENSITY_LTO10 => "LTO-10",
        _ => "Unknown",
    }
}

/// Return (min_bytes, max_bytes) for a given density code.
pub(crate) fn density_capacity_limits(code: u8) -> (u64, u64) {
    const GB: u64 = 1024 * 1024 * 1024;
    const TB: u64 = GB * 1024;
    match code {
        DENSITY_LTO5 => (GB, 3 * TB),
        DENSITY_LTO6 => (GB, 6 * TB),
        DENSITY_LTO7 => (GB, 15 * TB),
        DENSITY_LTO8 => (GB, 30 * TB),
        DENSITY_LTO9 => (GB, 45 * TB),
        DENSITY_LTO10 => (GB, 90 * TB),
        _ => (100 * 1024 * 1024, TB), // Default LTO: 100 MB .. 1 TB
    }
}
