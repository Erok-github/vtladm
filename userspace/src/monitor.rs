//! VTL monitor module: system performance, capacity trends, operation logs.
//!
//! Exposes JSON-serializable snapshots for the web dashboard.
//! All /proc reads are best-effort (Linux-only); non-Linux returns zeros.

use rusqlite::params;
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader};
use std::sync::Mutex;

use crate::init_db;

// ── System performance ──

#[derive(Debug, Serialize)]
pub struct CpuSample {
    pub pct: f64,       // 0.0–100.0
    pub num_cores: u32,
}

#[derive(Debug, Serialize)]
pub struct MemSample {
    pub total_kb: u64,
    pub used_kb: u64,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct DiskSample {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct SystemSnapshot {
    pub cpu: CpuSample,
    pub mem: MemSample,
    pub disks: Vec<DiskSample>,
}

fn read_proc_lines(path: &str) -> Vec<String> {
    fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .filter_map(|l| {
                    l.map_err(|e| eprintln!("monitor: read error on {}: {}", path, e)).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_proc_stat() -> (u64, u64) {
    // /proc/stat first line: cpu <user> <nice> <system> <idle> <iowait> <irq> <softirq> <steal> ...
    for line in read_proc_lines("/proc/stat") {
        if line.starts_with("cpu ") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 { continue; }
            let user: u64 = fields[1].parse().unwrap_or(0);
            let nice: u64 = fields[2].parse().unwrap_or(0);
            let system: u64 = fields[3].parse().unwrap_or(0);
            let idle: u64 = fields[4].parse().unwrap_or(0);
            let iowait: u64 = fields.get(5).and_then(|v| v.parse().ok()).unwrap_or(0);
            let irq: u64 = fields.get(6).and_then(|v| v.parse().ok()).unwrap_or(0);
            let softirq: u64 = fields.get(7).and_then(|v| v.parse().ok()).unwrap_or(0);
            let steal: u64 = fields.get(8).and_then(|v| v.parse().ok()).unwrap_or(0);
            let busy = user + nice + system + iowait + irq + softirq + steal;
            let total = busy + idle;
            return (busy, total);
        }
    }
    (0, 0)
}

fn core_count() -> u32 {
    read_proc_lines("/proc/cpuinfo")
        .iter()
        .filter(|l| l.starts_with("processor"))
        .count() as u32
}

static LAST_CPU: Mutex<Option<(u64, u64)>> = Mutex::new(None);

/// Take a CPU sample. Call this periodically; first call returns 0.
pub fn sample_cpu() -> CpuSample {
    let (busy, total) = parse_proc_stat();
    let cores = core_count().max(1);
    let mut last = LAST_CPU.lock().unwrap_or_else(|e| e.into_inner());
    let pct = match *last {
        Some((pb, pt)) => {
            let db = busy.saturating_sub(pb) as f64;
            let dt = total.saturating_sub(pt) as f64;
            if dt > 0.0 { (db / dt * 100.0).min(100.0 * cores as f64) } else { 0.0 }
        }
        None => 0.0,
    };
    *last = Some((busy, total));
    CpuSample { pct, num_cores: cores }
}

pub fn sample_mem() -> MemSample {
    let lines = read_proc_lines("/proc/meminfo");
    let mut total = 0u64;
    let mut free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    for line in &lines {
        let l = line.to_lowercase();
        let val = || {
            line.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        };
        if l.starts_with("memtotal:") { total = val(); }
        if l.starts_with("memfree:") { free = val(); }
        if l.starts_with("buffers:") { buffers = val(); }
        if l.starts_with("cached:") { cached = val(); }
    }
    let used = total.saturating_sub(free + buffers + cached);
    let pct = if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 };
    MemSample { total_kb: total, used_kb: used, pct }
}

pub fn sample_disks() -> Vec<DiskSample> {
    let lines = read_proc_lines("/proc/diskstats");
    let mut disks: Vec<DiskSample> = Vec::new();
    for line in &lines {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 14 { continue; }
        let name = f[2].to_string();
        // filter loop/ram devices, only real block devs
        if name.starts_with("loop") || name.starts_with("ram") { continue; }
        let read_sect: u64 = f[5].parse().unwrap_or(0);   // sectors read
        let write_sect: u64 = f[9].parse().unwrap_or(0);  // sectors written
        disks.push(DiskSample {
            name,
            read_bytes: read_sect * 512,
            write_bytes: write_sect * 512,
        });
    }
    disks
}

// ── Capacity trend ──

#[derive(Debug, Serialize)]
pub struct CapacityPoint {
    pub ts: String,
    pub library: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub tape_count: u32,
}

#[derive(Debug, Serialize)]
pub struct CapacityTrendResponse {
    pub points: Vec<CapacityPoint>,
}

/// Create the capacity_snapshots table. Safe to call multiple times.
pub fn ensure_capacity_snapshots_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS capacity_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL DEFAULT (datetime('now')),
            library TEXT NOT NULL,
            total_bytes INTEGER NOT NULL DEFAULT 0,
            used_bytes INTEGER NOT NULL DEFAULT 0,
            tape_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_cap_ts ON capacity_snapshots(ts);
        CREATE INDEX IF NOT EXISTS idx_cap_lib ON capacity_snapshots(library);
        COMMIT;",
    )
    .map_err(|e| e.to_string())
}

/// Take a snapshot of current library capacities.
pub fn snapshot_capacity() -> Result<(), String> {
    let conn = init_db().map_err(|e| e.to_string())?;
    ensure_capacity_snapshots_table(&conn)?;

    // collect per-library tape stats
    let mut stmt = conn
        .prepare(
            "SELECT l.name, COALESCE(SUM(t.capacity_bytes),0), COALESCE(SUM(t.used_bytes),0), COUNT(t.name)
             FROM libraries l LEFT JOIN tapes t ON t.library_id = l.id
             WHERE l.is_offline_storage = 0
             GROUP BY l.id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, u64, u64, u32)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u32,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for (lib, total, used, count) in &rows {
        conn.execute(
            "INSERT INTO capacity_snapshots (library, total_bytes, used_bytes, tape_count) VALUES (?1,?2,?3,?4)",
            params![lib, *total as i64, *used as i64, *count as i64],
        )
        .map_err(|e| e.to_string())?;
    }

    // Purge old snapshots (keep 90 days)
    let _ = conn.execute(
        "DELETE FROM capacity_snapshots WHERE ts < datetime('now', '-90 days')",
        [],
    );

    Ok(())
}

/// Read capacity trend for a library (or all). Returns last `limit` points.
pub fn get_capacity_trend(library: Option<&str>, limit: u32) -> Result<CapacityTrendResponse, String> {
    let conn = init_db().map_err(|e| e.to_string())?;
    ensure_capacity_snapshots_table(&conn)?;

    let points: Vec<CapacityPoint> = {
        if let Some(lib) = library {
            let mut stmt = conn
                .prepare(
                    "SELECT ts, library, total_bytes, used_bytes, tape_count
                     FROM capacity_snapshots WHERE library = ?1
                     ORDER BY ts DESC LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![lib, limit], |r| {
                Ok(CapacityPoint {
                    ts: r.get(0)?,
                    library: r.get(1)?,
                    total_bytes: r.get::<_, i64>(2)? as u64,
                    used_bytes: r.get::<_, i64>(3)? as u64,
                    tape_count: r.get::<_, i32>(4)? as u32,
                })
            })
            .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT ts, library, total_bytes, used_bytes, tape_count
                     FROM capacity_snapshots
                     ORDER BY ts DESC LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![limit], |r| {
                Ok(CapacityPoint {
                    ts: r.get(0)?,
                    library: r.get(1)?,
                    total_bytes: r.get::<_, i64>(2)? as u64,
                    used_bytes: r.get::<_, i64>(3)? as u64,
                    tape_count: r.get::<_, i32>(4)? as u32,
                })
            })
            .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        }
    };

    Ok(CapacityTrendResponse { points })
}

// ── Operation events ──

#[derive(Debug, Serialize)]
pub struct EventEntry {
    pub id: i64,
    pub ts: String,
    pub category: String,
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct EventsResponse {
    pub events: Vec<EventEntry>,
}

pub fn ensure_events_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS operation_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL DEFAULT (datetime('now')),
            category TEXT NOT NULL DEFAULT '',
            action TEXT NOT NULL DEFAULT '',
            detail TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_ev_ts ON operation_events(ts);
        CREATE INDEX IF NOT EXISTS idx_ev_cat ON operation_events(category);
        COMMIT;",
    )
    .map_err(|e| e.to_string())
}

/// Log an operation event. Safe to call anywhere with DB access.
#[allow(dead_code)]
pub fn log_event(category: &str, action: &str, detail: &str) {
    if let Ok(conn) = init_db() {
        let _ = ensure_events_table(&conn);
        let _ = conn.execute(
            "INSERT INTO operation_events (category, action, detail) VALUES (?1, ?2, ?3)",
            params![category, action, detail],
        );
    }
}

/// Query recent events.
pub fn get_events(limit: u32, category: Option<&str>) -> Result<EventsResponse, String> {
    let conn = init_db().map_err(|e| e.to_string())?;
    ensure_events_table(&conn)?;

    let events: Vec<EventEntry> = {
        if let Some(cat) = category {
            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, category, action, detail FROM operation_events
                     WHERE category = ?1 ORDER BY id DESC LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![cat, limit], |r| {
                Ok(EventEntry {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    category: r.get(2)?,
                    action: r.get(3)?,
                    detail: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = conn
                .prepare("SELECT id, ts, category, action, detail FROM operation_events ORDER BY id DESC LIMIT ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map(params![limit], |r| {
                Ok(EventEntry {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    category: r.get(2)?,
                    action: r.get(3)?,
                    detail: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
            rows.filter_map(|r| r.ok()).collect()
        }
    };

    Ok(EventsResponse { events })
}

/// Purge old events after 90 days.
pub fn purge_old_events() -> Result<usize, String> {
    let conn = init_db().map_err(|e| e.to_string())?;
    ensure_events_table(&conn)?;
    conn.execute(
        "DELETE FROM operation_events WHERE ts < datetime('now', '-90 days')",
        [],
    )
    .map_err(|e| e.to_string())
}

/// Combined system snapshot for /api/monitor/system
pub fn system_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        cpu: sample_cpu(),
        mem: sample_mem(),
        disks: sample_disks(),
    }
}
