//! ZFS zvol backend for virtual tape storage.
//!
//! Instead of sparse files, tapes can be backed by ZFS volumes (`zvol`).
//! This provides native compression, snapshots, and block-level integrity.
//!
//! Requirements: ZFS kernel module loaded, `zpool` created, `zfs` command available.
//!
//! zvol naming convention: `<pool>/vtladm/<library>/<tape_name>`
//! The zvol device path is: `/dev/zvol/<pool>/vtladm/<library>/<tape_name>`

use crate::{log_message, VtlError};
use std::path::PathBuf;
use std::process::Command;

/// Runs `zfs` command, returns (stdout, stderr) or error.
pub fn zfs(args: &[&str]) -> Result<(String, String), VtlError> {
    let output = Command::new("zfs")
        .args(args)
        .output()
        .map_err(|e| VtlError::IoError(std::io::Error::new(
            e.kind(),
            format!("zfs 命令执行失败: {}", e),
        )))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(VtlError::InvalidParameter(format!(
            "zfs {} 失败: {}",
            args.join(" "),
            if stderr.is_empty() { &stdout } else { &stderr }
        )));
    }
    Ok((stdout, stderr))
}

/// Check if a zvol exists.
pub fn zvol_exists(pool: &str, library: &str, name: &str) -> bool {
    let path = format!("{}/vtladm/{}/{}", pool, library, name);
    zfs(&["list", "-H", "-o", "name", &path]).is_ok()
}

/// Create a zvol for a virtual tape.
///
/// Returns the device path (`/dev/zvol/<pool>/vtladm/<library>/<name>`).
pub fn zvol_create(
    pool: &str,
    library: &str,
    name: &str,
    size_bytes: u64,
    block_size: u32,
) -> Result<PathBuf, VtlError> {
    let zvol_path = format!("{}/vtladm/{}", pool, library);
    let zvol_full = format!("{}/{}", zvol_path, name);
    let dev_path = format!("/dev/zvol/{}", zvol_full);

    // Ensure parent dataset exists
    if !zfs(&["list", "-H", "-o", "name", &zvol_path]).is_ok() {
        log_message(&format!("创建 zvol 父数据集: {}", zvol_path));
        zfs(&["create", "-p", &zvol_path])?;
    }

    // Remove existing zvol if present
    if zvol_exists(pool, library, name) {
        log_message(&format!("删除已存在的 zvol: {}", zvol_full));
        zfs(&["destroy", "-f", &zvol_full])?;
    }

    let size_str = format!("{}", size_bytes);
    let volblocksize_str = format!("{}", block_size.max(512));

    log_message(&format!(
        "创建 zvol: {} size={} volblocksize={}",
        zvol_full, size_str, volblocksize_str
    ));

    zfs(&[
        "create",
        "-V", &size_str,
        "-o", &format!("volblocksize={}", volblocksize_str),
        "-o", "compression=lz4",
        "-o", "sync=disabled", // VTL data integrity via tape labels, not per-block sync
        &zvol_full,
    ])?;

    Ok(PathBuf::from(dev_path))
}

/// Delete a zvol.
pub fn zvol_destroy(pool: &str, library: &str, name: &str) -> Result<(), VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    if !zvol_exists(pool, library, name) {
        return Ok(());
    }
    log_message(&format!("销毁 zvol: {}", zvol_full));
    zfs(&["destroy", "-f", &zvol_full])?;
    Ok(())
}

/// Resize a zvol to a new size in bytes.
#[allow(dead_code)]
pub fn zvol_resize(pool: &str, library: &str, name: &str, new_size: u64) -> Result<(), VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    let size_str = format!("{}", new_size);
    log_message(&format!("调整 zvol 大小: {} -> {}", zvol_full, size_str));
    zfs(&["set", &format!("volsize={}", size_str), &zvol_full])?;
    Ok(())
}

/// Get the current size of a zvol in bytes.
#[allow(dead_code)]
pub fn zvol_get_size(pool: &str, library: &str, name: &str) -> Result<u64, VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    let (stdout, _) = zfs(&["get", "-Hp", "-o", "value", "volsize", &zvol_full])?;
    stdout.parse::<u64>().map_err(|_| {
        VtlError::InvalidParameter(format!("无法解析 zvol 大小: {}", stdout))
    })
}

/// Create a snapshot of a zvol.
pub fn zvol_snapshot(
    pool: &str, library: &str, name: &str, snap_name: &str,
) -> Result<(), VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    let snap_full = format!("{}@{}", zvol_full, snap_name);
    log_message(&format!("创建 zvol 快照: {}", snap_full));
    zfs(&["snapshot", &snap_full])?;
    Ok(())
}

/// List snapshots for a zvol.
pub fn zvol_list_snapshots(
    pool: &str, library: &str, name: &str,
) -> Result<Vec<String>, VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    let (stdout, _) = zfs(&["list", "-H", "-o", "name", "-t", "snapshot"])?;
    let prefix = format!("{}@", zvol_full);
    Ok(stdout
        .lines()
        .filter(|l| l.starts_with(&prefix))
        .map(|l| l.trim_start_matches(&prefix).to_string())
        .collect())
}

/// Rollback a zvol to a named snapshot. Destroys any newer snapshots.
pub fn zvol_rollback(
    pool: &str, library: &str, name: &str, snap_name: &str,
) -> Result<(), VtlError> {
    let zvol_full = format!("{}/vtladm/{}/{}", pool, library, name);
    let snap_full = format!("{}@{}", zvol_full, snap_name);
    log_message(&format!("回滚 zvol 到快照: {}", snap_full));
    zfs(&["rollback", "-r", &snap_full])?;
    Ok(())
}

/// Return the device path for a zvol.
/// Format: /dev/zvol/<pool>/vtladm/<library>/<name>
#[allow(dead_code)]
pub fn zvol_device_path(pool: &str, library: &str, name: &str) -> PathBuf {
    PathBuf::from(format!("/dev/zvol/{}/vtladm/{}/{}", pool, library, name))
}

/// Initialize a zvol tape with zeroed content to a given size.
/// Writes zeros to the beginning to ensure proper block allocation.
#[allow(dead_code)]
pub fn zvol_zero_first_block(dev_path: &std::path::Path, block_size: u64) -> Result<(), VtlError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let mut f = OpenOptions::new()
        .write(true)
        .open(dev_path)
        .map_err(|e| VtlError::IoError(e))?;
    let zeros = vec![0u8; block_size as usize];
    f.write_all(&zeros)
        .map_err(VtlError::IoError)?;
    f.sync_all()
        .map_err(VtlError::IoError)?;
    Ok(())
}
