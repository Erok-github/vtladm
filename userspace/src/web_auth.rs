//! Web 登录：算术验证码、会话 cookie、bcrypt 密码文件（默认 admin，可修改）。

use bcrypt::{hash, verify, DEFAULT_COST};
use rand::seq::SliceRandom;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// 会话 `last_activity` 落盘的最小间隔（秒）。
/// 每次活动请求都回写 SQLite 会形成写放大，因此按此间隔节流。
const SESSION_TOUCH_MIN_INTERVAL_SECS: u64 = 60;

/// 当前 Unix 时间戳（秒），用于会话持久化的时间字段。
fn unix_now() -> i64 {
    chrono::Local::now().timestamp()
}

pub const DEFAULT_WEB_USER: &str = "admin";
/// Test-only password. Production deployments get a random UUID password
/// (see init_auth_file); this constant exists solely for #[cfg(test)]
/// login tests and is compiled out of release builds.
#[cfg(test)]
pub const DEFAULT_WEB_PASSWORD: &str = "admin";
/// Cookie `Max-Age` 与会话有效期（秒）。设为会话 cookie（浏览器关闭即失效），
/// 此常量仅用于服务端空闲超时判断。
pub const SESSION_SECS: u64 = 86400;
/// 空闲超时（秒）：超过此时间无活动则会话失效
const SESSION_IDLE_SECS: u64 = 1800;

const CAPTCHA_SECS: u64 = 300;
const LOGIN_WINDOW_SECS: u64 = 60;
const LOGIN_MAX_FAILURES: u32 = 5;
const LOGIN_LOCK_SECS: u64 = 300;
const MAX_SESSIONS: usize = 128;

#[derive(Debug, Clone)]
struct SessionData {
    username: String,
    created_at: Instant,
    last_activity: Instant,
    csrf_token: String,
    password_change_required: bool,
    /// 上次落盘的 `last_activity`（Unix 秒），用于节流回写。
    db_synced_unix: i64,
}

/// Web 会话的 SQLite 持久化存储（表 `web_sessions`）。
///
/// 会话令牌以明文落库：catalog 库文件由 `init_db()` 创建时 chmod 0600
/// （见 main.rs），文件权限与 `web_admin.json` 同级，风险可接受。
/// 所有方法吞错不 panic：持久化失败不应影响内存会话的正常使用。
struct SessionStore {
    conn: Mutex<Connection>,
    /// schema 是否已在本进程创建（`CREATE TABLE IF NOT EXISTS` 幂等，只需一次）。
    ready: Mutex<bool>,
}

impl SessionStore {
    /// 打开 catalog 库上的会话表。**db 文件不存在时返回 None**：setup 完成前
    /// web 层不得提前创建 catalog 库（`api_login` 是 public 路由，setup 前可达）。
    fn open(db_path: &Path) -> Option<Self> {
        if !db_path.exists() {
            return None;
        }
        let conn = Connection::open(db_path).ok()?;
        conn.busy_timeout(Duration::from_secs(2)).ok()?;
        Some(Self {
            conn: Mutex::new(conn),
            ready: Mutex::new(false),
        })
    }

    fn ensure_schema(&self) {
        let mut ready = self.ready.lock().unwrap();
        if *ready {
            return;
        }
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS web_sessions (
                token TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                csrf_token TEXT NOT NULL,
                password_change_required INTEGER NOT NULL DEFAULT 0,
                created_unix INTEGER NOT NULL,
                last_activity_unix INTEGER NOT NULL
            )",
            [],
        );
        *ready = true;
    }

    fn save(&self, token: &str, username: &str, csrf: &str, must_change: bool) {
        self.ensure_schema();
        let now = unix_now();
        let _ = self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO web_sessions
             (token, username, csrf_token, password_change_required, created_unix, last_activity_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![token, username, csrf, must_change as i32, now],
        );
    }

    fn touch(&self, token: &str, last_activity_unix: i64) {
        self.ensure_schema();
        let _ = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE web_sessions SET last_activity_unix = ?2 WHERE token = ?1",
                params![token, last_activity_unix],
            );
    }

    fn remove(&self, token: &str) {
        self.ensure_schema();
        let _ = self.conn.lock().unwrap().execute(
            "DELETE FROM web_sessions WHERE token = ?1",
            params![token],
        );
    }

    fn clear(&self) {
        self.ensure_schema();
        let _ = self
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM web_sessions", []);
    }

    /// 读回所有未过期会话，并顺手清理过期行（进程重启后 last_activity
    /// 以落库值为准恢复，避免内存态丢失导致空闲超时从头计算）。
    fn load_active(&self) -> Vec<(String, SessionData)> {
        self.ensure_schema();
        let now = unix_now();
        let cutoff_created = now - SESSION_SECS as i64;
        let cutoff_idle = now - SESSION_IDLE_SECS as i64;
        let _ = self.conn.lock().unwrap().execute(
            "DELETE FROM web_sessions WHERE created_unix < ?1 OR last_activity_unix < ?2",
            params![cutoff_created, cutoff_idle],
        );
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT token, username, csrf_token, password_change_required, created_unix, last_activity_unix
             FROM web_sessions WHERE created_unix >= ?1 AND last_activity_unix >= ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![cutoff_created, cutoff_idle], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        });
        let mut out = Vec::new();
        if let Ok(rows) = rows {
            let inst_now = Instant::now();
            for (tok, user, csrf, must, created, last) in rows.flatten() {
                let created_age = (now - created).max(0) as u64;
                let idle_age = (now - last).max(0) as u64;
                out.push((
                    tok,
                    SessionData {
                        username: user,
                        created_at: inst_now - Duration::from_secs(created_age),
                        last_activity: inst_now - Duration::from_secs(idle_age),
                        csrf_token: csrf,
                        password_change_required: must != 0,
                        db_synced_unix: last,
                    },
                ));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginFail {
    BadCredentials,
    AuthFile,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebAuthFile {
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub allow_iscsi_exec: bool,
    #[serde(default)]
    pub password_change_required: bool,
}

pub struct WebState {
    pub auth_file: PathBuf,
    captcha: Mutex<HashMap<String, (i32, Instant)>>,
    sessions: Mutex<HashMap<String, SessionData>>,
    login_failures: Mutex<HashMap<String, LoginThrottle>>,
    /// 会话持久化存储；None 表示纯内存（单测 / db 未就绪时）。
    session_store: Option<SessionStore>,
}

#[derive(Clone, Copy)]
struct LoginThrottle {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

#[derive(serde::Serialize)]
pub struct SessionInfo {
    pub token_prefix: String,
    pub username: String,
    pub created_secs_ago: u64,
    pub is_current: bool,
}

impl WebState {
    /// 纯内存会话（无持久化）。db 文件不存在 / 未就绪时自动降级为此模式。
    pub fn new(auth_file: PathBuf) -> Self {
        Self {
            auth_file,
            captcha: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            login_failures: Mutex::new(HashMap::new()),
            session_store: None,
        }
    }

    /// 带会话持久化的构造：会话写入 catalog 库的 `web_sessions` 表，
    /// 进程重启后可通过 `restore_sessions()` 恢复未过期会话。
    pub fn with_session_store(auth_file: PathBuf, db_path: &Path) -> Self {
        let mut st = Self::new(auth_file);
        st.session_store = SessionStore::open(db_path);
        st
    }

    /// 启动时从库中恢复未过期会话（并清理过期行）。应在 `init_auth_file` 之后调用。
    pub fn restore_sessions(&self) {
        let Some(store) = &self.session_store else {
            return;
        };
        let active = store.load_active();
        if active.is_empty() {
            return;
        }
        let mut sess = self.sessions.lock().unwrap();
        for (tok, sd) in active {
            sess.entry(tok).or_insert(sd);
        }
    }

    pub fn init_auth_file(&self) -> Result<(), std::io::Error> {
        if self.auth_file.exists() {
            return Ok(());
        }
        let random_pw = Uuid::new_v4().to_string();
        let msg = format!(
            "首次初始化认证文件，随机生成管理员密码: {} (请登录后立即修改)",
            random_pw
        );
        // 同时输出到 stdout 和 stderr，确保在 systemd/journalctl 场景下可见
        println!("vtladm: {}", msg);
        eprintln!("vtladm: {}", msg);
        super::log_message(&format!("首次初始化 web_admin.json，随机密码已生成"));
        self.write_default_auth_with_flags(&random_pw, true)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// 强制重写 `web_admin.json`（恢复默认用户 `admin` 与新密码哈希）。
    /// 必须提供密码，不接受空字符串或过短密码。
    pub fn force_reset_auth(&self, password: &str) -> Result<(), String> {
        if password.chars().count() < 8 {
            return Err("密码至少 8 个字符".into());
        }
        self.write_default_auth_with_flags(password, true)
    }

    /// 仅测试用：强制重写认证文件，不要求首次登录后改密。
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn force_reset_auth_no_pw_change(&self, password: &str) -> Result<(), String> {
        self.write_default_auth_with_flags(password, false)
    }

    fn write_default_auth_with_flags(&self, password: &str, require_change: bool) -> Result<(), String> {
        if let Some(p) = self.auth_file.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let password_hash = hash(password, DEFAULT_COST).map_err(|e| e.to_string())?;
        let cred = WebAuthFile {
            username: DEFAULT_WEB_USER.to_string(),
            password_hash,
            allow_iscsi_exec: false,
            password_change_required: require_change,
        };
        self.write_auth(&cred)
    }

    pub fn read_auth(&self) -> Result<WebAuthFile, String> {
        let s = fs::read_to_string(&self.auth_file).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }

    pub fn write_auth(&self, cred: &WebAuthFile) -> Result<(), String> {
        let tmp = self.auth_file.with_extension("tmp.json");
        let body = serde_json::to_string_pretty(cred).map_err(|e| e.to_string())?;
        fs::write(&tmp, body).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&tmp, &self.auth_file).map_err(|e| e.to_string())
    }

    pub fn new_captcha(&self) -> (String, String) {
        let mut cap = self.captcha.lock().unwrap();
        let now = Instant::now();
        cap.retain(|_, (_, t)| now.duration_since(*t) < Duration::from_secs(CAPTCHA_SECS + 60));

        let id = Uuid::new_v4().to_string();
        let a = rand::random::<u8>() % 9 + 1;
        let b = rand::random::<u8>() % 9 + 1;
        let (a, b) = (a.max(b), a.min(b));
        let ops = ["+", "-", "*"];
        let op = ops.choose(&mut rand::thread_rng()).copied().unwrap();
        let ans = match op {
            "+" => a as i32 + b as i32,
            "-" => a as i32 - b as i32,
            "*" => a as i32 * b as i32,
            _ => unreachable!(),
        };
        let question = format!("{} {} {} = ?", a, op, b);
        cap.insert(id.clone(), (ans, Instant::now()));
        (id, question)
    }

    pub fn verify_captcha(&self, id: &str, answer_str: &str) -> bool {
        let Ok(ans) = answer_str.trim().parse::<i32>() else {
            return false;
        };
        let mut cap = self.captcha.lock().unwrap();
        if let Some((expect, t)) = cap.remove(id) {
            if t.elapsed() > Duration::from_secs(CAPTCHA_SECS) {
                return false;
            }
            return ans == expect;
        }
        false
    }

    pub fn login(&self, user: &str, pass: &str) -> Result<(String, String), LoginFail> {
        let cred = self.read_auth().map_err(|_| LoginFail::AuthFile)?;
        if user != cred.username {
            return Err(LoginFail::BadCredentials);
        }
        match verify(pass, &cred.password_hash) {
            Ok(true) => { /* 密码正确，继续 */ }
            Ok(false) => return Err(LoginFail::BadCredentials),
            Err(e) => {
                // bcrypt 解析哈希失败属于罕见异常（认证文件被破坏 / 哈希被改坏）。
                super::log_error(
                    "web_auth::login",
                    &format!(
                        "bcrypt verify 异常: {} (建议: vtladm reset-web-auth)",
                        e
                    ),
                );
                return Err(LoginFail::BadCredentials);
            }
        }
        let tok = Uuid::new_v4().to_string();
        let csrf = Uuid::new_v4().to_string();
        let must_change = cred.password_change_required;
        let mut sess = self.sessions.lock().unwrap();
        let now = Instant::now();
        sess.retain(|_, sd| now.duration_since(sd.created_at) < Duration::from_secs(SESSION_SECS + 3600));
        let mut evicted: Option<String> = None;
        if sess.len() >= MAX_SESSIONS {
            if let Some(oldest) = sess
                .iter()
                .min_by_key(|(_, sd)| sd.created_at)
                .map(|(tok, _)| tok.clone())
            {
                sess.remove(&oldest);
                evicted = Some(oldest);
            }
        }
        sess.insert(
            tok.clone(),
            SessionData {
                username: user.to_string(),
                created_at: Instant::now(),
                last_activity: Instant::now(),
                csrf_token: csrf.clone(),
                password_change_required: must_change,
                db_synced_unix: unix_now(),
            },
        );
        drop(sess);
        if let Some(store) = &self.session_store {
            store.save(&tok, user, &csrf, must_change);
            // 被淘汰的会话同步从库中删除，避免重启后"复活"
            if let Some(old) = evicted {
                store.remove(&old);
            }
        }
        Ok((tok, csrf))
    }

    pub fn login_allowed(&self, key: &str) -> Result<(), Duration> {
        let now = Instant::now();
        let mut failures = self.login_failures.lock().unwrap();
        failures.retain(|_, v| {
            v.locked_until.map(|t| t > now).unwrap_or(false)
                || now.duration_since(v.window_start) < Duration::from_secs(LOGIN_WINDOW_SECS)
        });
        if let Some(v) = failures.get(key) {
            if let Some(until) = v.locked_until {
                if until > now {
                    return Err(until.duration_since(now));
                }
            }
        }
        Ok(())
    }

    pub fn record_login_failure(&self, key: &str) {
        let now = Instant::now();
        let mut failures = self.login_failures.lock().unwrap();
        let entry = failures.entry(key.to_string()).or_insert(LoginThrottle {
            failures: 0,
            window_start: now,
            locked_until: None,
        });
        if now.duration_since(entry.window_start) >= Duration::from_secs(LOGIN_WINDOW_SECS) {
            entry.failures = 0;
            entry.window_start = now;
            entry.locked_until = None;
        }
        entry.failures += 1;
        if entry.failures >= LOGIN_MAX_FAILURES {
            entry.locked_until = Some(now + Duration::from_secs(LOGIN_LOCK_SECS));
        }
    }

    pub fn record_login_success(&self, key: &str) {
        self.login_failures.lock().unwrap().remove(key);
    }

    pub fn session_username(&self, token: Option<&str>) -> Option<String> {
        let t = token?;
        // (用户名, 是否需节流回写 last_activity, 回写时间戳)；
        // expired=true 表示会话已过期并在内存中删除，需同步删库。
        let (username, needs_touch, touch_unix, expired) = {
            let mut sess = self.sessions.lock().unwrap();
            if let Some(sd) = sess.get_mut(t) {
                let expired = sd.created_at.elapsed() > Duration::from_secs(SESSION_SECS)
                    || sd.last_activity.elapsed() > Duration::from_secs(SESSION_IDLE_SECS);
                if expired {
                    sess.remove(t);
                    (String::new(), false, 0, true)
                } else {
                    sd.last_activity = Instant::now();
                    let now_unix = unix_now();
                    let needs_touch = now_unix.saturating_sub(sd.db_synced_unix)
                        >= SESSION_TOUCH_MIN_INTERVAL_SECS as i64;
                    if needs_touch {
                        sd.db_synced_unix = now_unix;
                    }
                    (sd.username.clone(), needs_touch, now_unix, false)
                }
            } else {
                return None;
            }
        };
        // 锁外再做 SQLite 调用，避免持锁跨 IO。
        if let Some(store) = &self.session_store {
            if expired {
                store.remove(t);
            } else if needs_touch {
                store.touch(t, touch_unix);
            }
        }
        if expired {
            return None;
        }
        Some(username)
    }

    /// Check if a session token is valid without updating last_activity (for ping endpoints).
    pub fn session_is_valid(&self, token: Option<&str>) -> bool {
        let t = match token {
            Some(t) => t,
            None => return false,
        };
        let mut sess = self.sessions.lock().unwrap();
        if let Some(sd) = sess.get(t) {
            let elapsed = sd.created_at.elapsed();
            if elapsed > Duration::from_secs(SESSION_SECS) {
                sess.remove(t);
                return false;
            }
            let idle = sd.last_activity.elapsed();
            if idle > Duration::from_secs(SESSION_IDLE_SECS) {
                sess.remove(t);
                return false;
            }
            return true;
        }
        false
    }

    /// 查询会话的 CSRF token。
    pub fn csrf_token_for_session(&self, token: &str) -> Option<String> {
        self.sessions
            .lock()
            .unwrap()
            .get(token)
            .map(|sd| sd.csrf_token.clone())
    }

    /// 查询会话是否要求改密（来自认证文件的快照）。
    pub fn session_must_change_password(&self, token: &str) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(token)
            .map(|sd| sd.password_change_required)
            .unwrap_or(false)
    }

    /// 登录时检查认证文件是否要求改密。
    pub fn must_change_password(&self) -> bool {
        self.read_auth()
            .map(|c| c.password_change_required)
            .unwrap_or(false)
    }

    pub fn list_sessions(&self, current_token: &str) -> Vec<SessionInfo> {
        let now = Instant::now();
        let mut sess = self.sessions.lock().unwrap();
        sess.retain(|_, sd| {
            now.duration_since(sd.created_at) < Duration::from_secs(SESSION_SECS + 3600)
        });
        sess.iter()
            .map(|(tok, sd)| SessionInfo {
                token_prefix: tok.chars().take(8).collect::<String>() + "...",
                username: sd.username.clone(),
                created_secs_ago: now.duration_since(sd.created_at).as_secs(),
                is_current: tok == current_token,
            })
            .collect()
    }

    /// 清除所有活跃会话（密码重置后调用）。
    pub fn clear_all_sessions(&self) {
        self.sessions.lock().unwrap().clear();
        if let Some(store) = &self.session_store {
            store.clear();
        }
    }

    pub fn revoke_session(&self, token_prefix: &str, current_token: &str) -> Result<(), String> {
        if current_token.starts_with(token_prefix) {
            return Err("不能撤销当前会话，请使用登出".into());
        }
        let removed: Option<String> = {
            let mut sess = self.sessions.lock().unwrap();
            if sess.remove(token_prefix).is_some() {
                Some(token_prefix.to_string())
            } else {
                let matches: Vec<String> = sess
                    .keys()
                    .filter(|k| k.starts_with(token_prefix))
                    .cloned()
                    .collect();
                if matches.len() == 1 {
                    if matches[0] == current_token {
                        return Err("不能撤销当前会话，请使用登出".into());
                    }
                    sess.remove(&matches[0]);
                    Some(matches[0].clone())
                } else {
                    None
                }
            }
        };
        match removed {
            Some(full_token) => {
                // 按前缀匹配到的完整令牌同步从库中删除
                if let Some(store) = &self.session_store {
                    store.remove(&full_token);
                }
                Ok(())
            }
            None => {
                let matches_len = self
                    .sessions
                    .lock()
                    .unwrap()
                    .keys()
                    .filter(|k| k.starts_with(token_prefix))
                    .count();
                if matches_len > 1 {
                    Err("匹配到多个会话，请提供更完整的前缀".into())
                } else {
                    Err("未找到匹配的会话".into())
                }
            }
        }
    }

    pub fn logout_token(&self, token: &str) {
        self.sessions.lock().unwrap().remove(token);
        if let Some(store) = &self.session_store {
            store.remove(token);
        }
    }

    #[cfg(test)]
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    #[cfg(test)]
    pub fn insert_test_session(&self, token: &str, user: &str, age_secs: u64) {
        self.sessions.lock().unwrap().insert(
            token.to_string(),
            SessionData {
                username: user.to_string(),
                created_at: Instant::now() - Duration::from_secs(age_secs),
                last_activity: Instant::now(),
                csrf_token: format!("csrf_{}", token),
                password_change_required: false,
                db_synced_unix: unix_now(),
            },
        );
    }

    pub fn change_password(&self, old_p: &str, new_p: &str) -> Result<(), String> {
        if new_p.chars().count() < 8 {
            return Err("新密码至少 8 个字符".into());
        }
        let mut cred = self.read_auth()?;
        if !verify(old_p, &cred.password_hash).unwrap_or(false) {
            return Err("原密码不正确".into());
        }
        cred.password_hash = hash(new_p, DEFAULT_COST).map_err(|e| e.to_string())?;
        cred.password_change_required = false;
        self.write_auth(&cred)?;
        self.sessions.lock().unwrap().clear();
        if let Some(store) = &self.session_store {
            store.clear();
        }
        Ok(())
    }

    pub fn allow_iscsi_exec(&self) -> bool {
        self.read_auth()
            .map(|c| c.allow_iscsi_exec)
            .unwrap_or(false)
    }

    pub fn set_allow_iscsi_exec(&self, allow: bool) -> Result<(), String> {
        let mut cred = self.read_auth()?;
        cred.allow_iscsi_exec = allow;
        self.write_auth(&cred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TEST_PASSWORD: &str = "test-password-4test";

    fn tmp_auth_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vtladm_web_auth_{}_{}_{}",
            label,
            std::process::id(),
            rand::random::<u32>()
        ))
    }

    fn fresh_state_with_password(label: &str, password: &str) -> WebState {
        let p = tmp_auth_path(label);
        let _ = fs::remove_file(&p);
        let st = WebState::new(p);
        st.write_default_auth_with_flags(password, true).unwrap();
        st
    }

    fn fresh_state(label: &str) -> WebState {
        fresh_state_with_password(label, TEST_PASSWORD)
    }

    /// 与 `new_captcha` 的 `format!("{} {} {} = ?", …)` 一致。
    fn captcha_expected_answer(question: &str) -> i32 {
        let parts: Vec<&str> = question.split_whitespace().collect();
        assert_eq!(parts.len(), 5, "expected `a op b = ?`, got {:?}", parts);
        let a: i32 = parts[0].parse().expect("a");
        let op = parts[1];
        let b: i32 = parts[2].parse().expect("b");
        assert_eq!(parts[3], "=");
        match op {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            _ => panic!("unknown op {:?}", op),
        }
    }

    #[test]
    fn test_web_auth_init_creates_json_with_defaults() {
        let st = fresh_state("init");
        let cred = st.read_auth().unwrap();
        assert_eq!(cred.username, DEFAULT_WEB_USER);
        assert!(cred.password_hash.starts_with("$2"));
        assert!(!cred.allow_iscsi_exec);
        assert!(cred.password_change_required);
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_login_default_password_and_session() {
        let st = fresh_state("login");
        let (tok, csrf) = st
            .login(DEFAULT_WEB_USER, TEST_PASSWORD)
            .expect("login");
        assert!(!csrf.is_empty());
        assert_eq!(
            st.session_username(Some(tok.as_str())).as_deref(),
            Some(DEFAULT_WEB_USER)
        );
        assert_eq!(st.csrf_token_for_session(&tok).as_deref(), Some(csrf.as_str()));
        assert!(st.session_must_change_password(&tok));
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_set_allow_iscsi_exec_roundtrip() {
        let st = fresh_state("iscsi_allow");
        assert!(!st.allow_iscsi_exec());
        st.set_allow_iscsi_exec(true).unwrap();
        assert!(st.allow_iscsi_exec());
        let cred = st.read_auth().unwrap();
        assert!(cred.allow_iscsi_exec);
        st.set_allow_iscsi_exec(false).unwrap();
        assert!(!st.allow_iscsi_exec());
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_login_wrong_password() {
        let st = fresh_state("badpw");
        assert_eq!(
            st.login(DEFAULT_WEB_USER, "wrong-password-here"),
            Err(LoginFail::BadCredentials)
        );
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_captcha_correct_answer_accepted() {
        let st = fresh_state("cap_ok");
        for _ in 0..24 {
            let (id, q) = st.new_captcha();
            let ans = captcha_expected_answer(&q);
            assert!(st.verify_captcha(&id, &ans.to_string()), "question={}", q);
        }
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_captcha_wrong_answer_rejected() {
        let st = fresh_state("cap_bad");
        let (id, _q) = st.new_captcha();
        assert!(!st.verify_captcha(&id, "999999"));
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_captcha_double_verify_consumes_entry() {
        let st = fresh_state("cap_twice");
        let (id, q) = st.new_captcha();
        let ans = captcha_expected_answer(&q);
        assert!(st.verify_captcha(&id, &ans.to_string()));
        assert!(!st.verify_captcha(&id, &ans.to_string()));
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_change_password_roundtrip() {
        let st = fresh_state("chgpw");
        let (old_tok, _) = st
            .login(DEFAULT_WEB_USER, TEST_PASSWORD)
            .expect("old pw");
        st.change_password(TEST_PASSWORD, "newpass-8chars-min")
            .unwrap();
        assert!(st.session_username(Some(old_tok.as_str())).is_none());
        assert_eq!(
            st.login(DEFAULT_WEB_USER, TEST_PASSWORD),
            Err(LoginFail::BadCredentials)
        );
        let (tok, _) = st
            .login(DEFAULT_WEB_USER, "newpass-8chars-min")
            .expect("new pw");
        assert!(st.session_username(Some(tok.as_str())).is_some());
        // 改密后 password_change_required 应被清除
        assert!(!st.must_change_password());
        assert!(!st.session_must_change_password(&tok));
        assert!(st.change_password("newpass-8chars-min", "short").is_err());
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_login_rate_limit_and_clear() {
        let st = fresh_state("rate");
        let key = "203.0.113.10";
        for _ in 0..5 {
            assert!(st.login_allowed(key).is_ok());
            st.record_login_failure(key);
        }
        assert!(st.login_allowed(key).is_err());
        st.record_login_success(key);
        assert!(st.login_allowed(key).is_ok());
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_session_cap() {
        let st = fresh_state("session_cap");
        st.insert_test_session("oldest", DEFAULT_WEB_USER, 1_000);
        for i in 1..MAX_SESSIONS {
            st.insert_test_session(&format!("tok{i}"), DEFAULT_WEB_USER, 10);
        }
        assert_eq!(st.session_count(), MAX_SESSIONS);
        let (tok, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        assert!(st.session_count() <= 128);
        assert!(st.session_username(Some("oldest")).is_none());
        assert!(st.session_username(Some(tok.as_str())).is_some());
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_wrong_username_rejected() {
        let st = fresh_state("user");
        assert_eq!(
            st.login("not-admin", TEST_PASSWORD),
            Err(LoginFail::BadCredentials)
        );
        let _ = fs::remove_file(&st.auth_file);
    }

    #[test]
    fn test_web_auth_list_revoke_sessions() {
        let st = fresh_state("list");
        let (tok1, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        let (tok2, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        let sessions = st.list_sessions(&tok1);
        assert_eq!(sessions.len(), 2);
        let current = sessions.iter().find(|s| s.is_current).unwrap();
        assert!(current.token_prefix.starts_with(&tok1[..8]));
        // 撤销 tok2
        st.revoke_session(&tok2[..16], &tok1).unwrap();
        assert_eq!(st.list_sessions(&tok1).len(), 1);
        // 不能撤销自己
        assert!(st.revoke_session(&tok1[..16], &tok1).is_err());
        let _ = fs::remove_file(&st.auth_file);
    }

    fn tmp_db_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vtladm_web_sess_{}_{}_{}.db",
            label,
            std::process::id(),
            rand::random::<u32>()
        ))
    }

    /// 建一个带持久化的 state；db 文件先由调用方创建（SessionStore::open
    /// 要求文件已存在，避免提前创建 catalog 库）。
    fn fresh_state_with_store(label: &str) -> (WebState, PathBuf, PathBuf) {
        let auth = tmp_auth_path(label);
        let _ = fs::remove_file(&auth);
        let db = tmp_db_path(label);
        let _ = fs::remove_file(&db);
        Connection::open(&db).unwrap();
        let st = WebState::with_session_store(auth.clone(), &db);
        st.write_default_auth_with_flags(TEST_PASSWORD, true).unwrap();
        (st, auth, db)
    }

    fn db_session_count(db: &Path) -> i64 {
        let conn = Connection::open(db).unwrap();
        conn.query_row("SELECT COUNT(*) FROM web_sessions", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn test_session_persist_and_restore_after_restart() {
        let (st, auth, db) = fresh_state_with_store("persist");
        let (tok, csrf) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        assert_eq!(db_session_count(&db), 1);
        // 模拟重启：新 WebState 指向同一 db，恢复会话
        let st2 = WebState::with_session_store(auth.clone(), &db);
        st2.restore_sessions();
        assert_eq!(
            st2.session_username(Some(tok.as_str())).as_deref(),
            Some(DEFAULT_WEB_USER)
        );
        assert_eq!(st2.csrf_token_for_session(&tok).as_deref(), Some(csrf.as_str()));
        assert_eq!(st2.list_sessions(&tok).len(), 1);
        let _ = fs::remove_file(&auth);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_session_store_prunes_expired_rows_on_restore() {
        let (st, auth, db) = fresh_state_with_store("expired");
        let (_tok, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        // 直接把行改老：created 与 last_activity 都超过上限
        let conn = Connection::open(&db).unwrap();
        let ancient = unix_now() - (SESSION_SECS as i64 + 60);
        conn.execute(
            "UPDATE web_sessions SET created_unix = ?1, last_activity_unix = ?1",
            params![ancient],
        )
        .unwrap();
        drop(conn);
        let st2 = WebState::with_session_store(auth.clone(), &db);
        st2.restore_sessions();
        // 过期会话不再恢复，且行已被清理
        assert_eq!(st2.session_count(), 0);
        assert_eq!(db_session_count(&db), 0);
        let _ = fs::remove_file(&auth);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_session_store_remove_and_clear_sync() {
        let (st, auth, db) = fresh_state_with_store("lifecycle");
        let (tok, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        // logout 同步删库
        st.logout_token(&tok);
        assert_eq!(db_session_count(&db), 0);
        // 两个会话 → clear_all_sessions 全清
        st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        let (tok2, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        assert_eq!(db_session_count(&db), 2);
        st.revoke_session(&tok2[..16], "current-not-tok2").unwrap();
        assert_eq!(db_session_count(&db), 1);
        st.clear_all_sessions();
        assert_eq!(db_session_count(&db), 0);
        let _ = fs::remove_file(&auth);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn test_with_session_store_missing_db_falls_back_to_memory() {
        let auth = tmp_auth_path("no_db");
        let _ = fs::remove_file(&auth);
        let missing = tmp_db_path("definitely_missing");
        let _ = fs::remove_file(&missing);
        let st = WebState::with_session_store(auth.clone(), &missing);
        st.write_default_auth_with_flags(TEST_PASSWORD, true).unwrap();
        // db 不存在：不得创建文件，会话纯内存
        assert!(!missing.exists());
        let (tok, _) = st.login(DEFAULT_WEB_USER, TEST_PASSWORD).unwrap();
        assert_eq!(
            st.session_username(Some(tok.as_str())).as_deref(),
            Some(DEFAULT_WEB_USER)
        );
        let _ = fs::remove_file(&auth);
    }
}
