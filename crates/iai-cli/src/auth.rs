//! 控制台访问控制：密码哈希（argon2id，存独立文件）+ 会话 token（存 SQLite）。
//!
//! 设计要点：
//! - **密码** 单独存到 `$IAI_HOME/console_auth.json`（权限 0600），与 SQLite 解耦：
//!   - 改密码只动一个文件，不影响业务数据；
//!   - 清除密码直接删文件即可，无 DB 依赖。
//! - 首次启动时若未设置密码，**自动生成强随机密码**，并把明文写入一次性文件
//!   `$IAI_HOME/CONSOLE_PASSWORD.txt`（0600）。`iai password set/reset` 会重写/删除该文件。
//! - **Session token** 32 字节熵 → 64 字符十六进制；服务端只存 `sha256(token)`，
//!   即使 SQLite 被读也拿不到明文 token。
//! - 默认 TTL 24 小时；校验时顺带清理过期项。
//! - argon2id 参数采用 OWASP 2024 推荐基线（m=19 MiB, t=2, p=1），单次 hash ≈ 50ms。

use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::{distributions::Alphanumeric, Rng, RngCore};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::storage;

/// Session 默认有效期（秒）。
pub const SESSION_TTL_SECS: i64 = 24 * 3600;

/// 控制台密码文件：`$IAI_HOME/console_auth.json`。
pub fn password_file() -> PathBuf {
    storage::data_dir().join("console_auth.json")
}

/// 一次性明文密码文件：`$IAI_HOME/CONSOLE_PASSWORD.txt`。
pub fn initial_password_file() -> PathBuf {
    storage::data_dir().join("CONSOLE_PASSWORD.txt")
}

/// 持久化的密码记录（PHC 字符串）。
#[derive(Serialize, Deserialize)]
struct PasswordRecord {
    /// argon2id PHC 字符串：`$argon2id$v=19$m=...,t=...,p=...$<salt>$<hash>`。
    phc: String,
    /// 设置时间（UTC 秒）。
    created_at: i64,
}

/// 是否已设置过密码（不读内容只看文件存在）。
pub fn is_password_set() -> bool {
    password_file().exists()
}

/// 是否还存在一次性明文文件（启动时生成的初始密码未迁移走）。
pub fn has_initial_password_file() -> bool {
    initial_password_file().exists()
}

/// 读取密码文件并解析。
fn read_record() -> Result<Option<PasswordRecord>> {
    let path = password_file();
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("读取密码文件失败: {}", path.display()))?;
    let rec: PasswordRecord = serde_json::from_slice(&bytes)
        .with_context(|| format!("解析密码文件失败: {}", path.display()))?;
    Ok(Some(rec))
}

/// 生成强随机密码：18 位 URL-safe 字母数字（去掉易混的 0/O/1/l/I）。
///
/// 用 `rand::distributions::Alphanumeric` 之上手动剔除约 8 个字符，
/// 实际字符集约 54 个 → 18 位 ≈ 100 bit 熵，远超爆破成本。
pub fn generate_strong_password() -> String {
    const CHARSET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..18)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// 设置（覆盖）控制台访问密码。密码长度 ≥ 8。
///
/// 同时清除一次性明文文件（设新密码意味着初始密码失效）。
pub fn set_password(plain: &str) -> Result<()> {
    if plain.len() < 8 {
        anyhow::bail!("密码至少 8 位");
    }
    let salt = SaltString::generate(&mut OsRng);
    let phc = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("密码哈希失败: {e}"))?
        .to_string();

    let rec = PasswordRecord { phc, created_at: storage::now_epoch() };
    let json = serde_json::to_vec_pretty(&rec).context("序列化密码记录失败")?;

    let path = password_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    write_secret(&tmp, &json)?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("重命名密码文件失败: {}", path.display()))?;
    // 设新密码后清掉一次性明文文件，避免遗留。
    let _ = std::fs::remove_file(initial_password_file());
    Ok(())
}

/// 校验密码。文件不存在时返回 false（未设密码）。
pub fn verify_password(plain: &str) -> bool {
    let Some(rec) = read_record().ok().flatten() else {
        return false;
    };
    let Ok(hash) = PasswordHash::new(&rec.phc) else {
        return false;
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &hash)
        .is_ok()
}

/// 重置密码为新的随机密码：清旧 session、生成新密码、写密码文件、
/// 同时把明文写回一次性文件供 admin 取走。
///
/// 返回新密码的明文（调用方可选择直接返回给用户）。
pub fn reset_to_random() -> Result<String> {
    let new_plain = generate_strong_password();
    set_password(&new_plain)?;
    write_initial_password(&new_plain)?;
    let conn = storage::open_conn()?;
    conn.execute("DELETE FROM auth_sessions", [])
        .context("清除所有 session 失败")?;
    Ok(new_plain)
}

/// 启动期保证：若未设密码则自动生成一个，并把明文写到一次性文件。
///
/// 返回 `Some(plain)` 表示本次确实生成了新密码（应提示用户）；
/// 返回 `None` 表示已有密码、什么都没做。
pub fn ensure_password_on_first_run() -> Result<Option<String>> {
    if is_password_set() {
        return Ok(None);
    }
    let plain = generate_strong_password();
    set_password(&plain)?;
    write_initial_password(&plain)?;
    Ok(Some(plain))
}

/// 把明文密码写到一次性文件（0600）。
fn write_initial_password(plain: &str) -> Result<()> {
    let path = initial_password_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# IAI Chain · 控制台初始密码（一次性文件，看完请删除！）\n\
         # 此文件由 iai 自动生成，下次 `iai password set` / `iai password reset` 时会被删除。\n\
         # 当前明文：\n\
         {plain}\n"
    );
    let tmp = path.with_extension("txt.tmp");
    write_secret(&tmp, body.as_bytes())?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// 读取一次性明文密码（如存在）。
pub fn read_initial_password() -> Option<String> {
    let path = initial_password_file();
    let body = std::fs::read_to_string(&path).ok()?;
    // 跳过注释行，提取真正的密码行
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|s| s.to_string())
}

/// 主动删除一次性明文文件。
pub fn delete_initial_password_file() -> Result<()> {
    let path = initial_password_file();
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("删除明文密码文件失败: {}", path.display()))?;
    }
    Ok(())
}

/// 以 0600 权限写入文件（仅 Unix）。
fn write_secret(path: &PathBuf, body: &[u8]) -> Result<()> {
    std::fs::write(path, body)
        .with_context(|| format!("写入文件失败: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(path, perm)?;
    }
    Ok(())
}

/// 生成 32 字节随机 token，转为 64 字符小写十六进制。
fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// 对 token 算 sha256（hex），用作服务端存储键。
fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

/// 创建新 session，返回 (token明文, expires_at_epoch)。
/// 调用方负责把明文 token 透传给客户端。
pub fn create_session(conn: &Connection) -> Result<(String, i64)> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = storage::now_epoch();
    let expires_at = now + SESSION_TTL_SECS;
    conn.execute(
        "INSERT INTO auth_sessions (token_hash, expires_at, created_at) VALUES (?, ?, ?)",
        params![token_hash, expires_at, now],
    )
    .context("写入 session 失败")?;
    Ok((token, expires_at))
}

/// 校验 session token 是否存在且未过期。顺带清理过期项。
pub fn validate_session(conn: &Connection, token: &str) -> bool {
    let now = storage::now_epoch();
    let _ = conn.execute(
        "DELETE FROM auth_sessions WHERE expires_at < ?",
        params![now],
    );

    let token_hash = hash_token(token);
    let hit = conn
        .query_row(
            "SELECT expires_at FROM auth_sessions WHERE token_hash = ?",
            params![token_hash],
            |r| r.get::<_, i64>(0),
        )
        .ok();
    match hit {
        Some(exp) if exp > now => true,
        _ => false,
    }
}

/// 注销当前 token（删表项）。
pub fn delete_session(conn: &Connection, token: &str) -> Result<()> {
    let token_hash = hash_token(token);
    conn.execute(
        "DELETE FROM auth_sessions WHERE token_hash = ?",
        params![token_hash],
    )
    .context("删除 session 失败")?;
    Ok(())
}

/// 主动清理所有过期 session（运维可手动调用）。
pub fn cleanup_expired_sessions(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM auth_sessions WHERE expires_at < ?",
        params![storage::now_epoch()],
    )?;
    Ok(n)
}

/// 标记为允许的 Alphanumeric（防止误引入依赖）。
#[allow(dead_code)]
fn _force_use_alphanumeric() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}
