use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use crate::get_config;

/// 日志轮转深度：`name.1` … `name.5`
const LOG_ROTATE_DEPTH: u32 = 5;

/// Global lock serialises log rotation + append to prevent interleaving / TOCTOU.
static LOG_LOCK: Mutex<()> = Mutex::new(());

fn maybe_rotate_log_file(
    log_dir: &Path,
    file_name: &str,
    line_len: u64,
    max_bytes: u64,
) -> std::io::Result<()> {
    let max_bytes = max_bytes.max(4096);
    let path = log_dir.join(file_name);
    if !path.exists() {
        return Ok(());
    }
    let len = fs::metadata(&path)?.len();
    if len.saturating_add(line_len) <= max_bytes {
        return Ok(());
    }

    let oldest = log_dir.join(format!("{}.{}", file_name, LOG_ROTATE_DEPTH));
    let _ = fs::remove_file(&oldest);
    for i in (1..LOG_ROTATE_DEPTH).rev() {
        let from = log_dir.join(format!("{}.{}", file_name, i));
        let to = log_dir.join(format!("{}.{}", file_name, i + 1));
        if from.exists() {
            if to.exists() {
                fs::remove_file(&to)?;
            }
            fs::rename(&from, &to)?;
        }
    }
    let first_rot = log_dir.join(format!("{}.1", file_name));
    if first_rot.exists() {
        fs::remove_file(&first_rot)?;
    }
    fs::rename(&path, &first_rot)?;
    Ok(())
}

fn try_append_log_line_in(
    log_dir: &Path,
    file_name: &str,
    line: &str,
    max_bytes: u64,
) -> std::io::Result<()> {
    fs::create_dir_all(log_dir)?;
    let line_len = line.len() as u64 + 1;
    maybe_rotate_log_file(log_dir, file_name, line_len, max_bytes)?;
    let path = log_dir.join(file_name);
    let mut file = File::options().append(true).create(true).open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    writeln!(file, "{}", line)
}

pub(crate) fn log_message(msg: &str) {
    let _guard = LOG_LOCK.lock().unwrap();
    let config = get_config();
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("[{}] {}", timestamp, msg);
    if let Err(e) =
        try_append_log_line_in(&config.log_dir, "vtladm.log", &line, config.log_max_bytes)
    {
        eprintln!("Warning: Failed to write to log: {}", e);
    }
}

pub(crate) fn log_error(msg: &str, error: &str) {
    let _guard = LOG_LOCK.lock().unwrap();
    let config = get_config();
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let log_entry = format!("[{}] ERROR: {} - {}", timestamp, msg, error);
    match try_append_log_line_in(
        &config.log_dir,
        "vtladm_errors.log",
        &log_entry,
        config.log_max_bytes,
    ) {
        Ok(()) => eprintln!("{}", log_entry),
        Err(e) => eprintln!("Warning: Failed to write to error log: {}", e),
    }
}
