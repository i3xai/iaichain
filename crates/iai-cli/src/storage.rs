//! 本地 SQLite 存储脚手架（rusqlite，bundled）。
//!
//! 阶段 0：仅建立数据库文件与一张 `schema_migrations` 迁移记录表，作为后续阶段
//! （节点身份 / 账本 / 市场 / 任务）建表的落点。账本表将在阶段 2 设计为 append-only + 哈希链。

use anyhow::Context;
use rusqlite::Connection;
use std::path::PathBuf;

/// 数据目录：`$IAI_HOME` 优先，否则 `~/.iai`，再否则当前目录下 `.iai`。
pub fn data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("IAI_HOME") {
        return PathBuf::from(home);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".iai");
    }
    PathBuf::from(".iai")
}

/// 打开（必要时创建）数据库，应用基础迁移，返回连接。
pub fn init_db() -> anyhow::Result<Connection> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建数据目录失败: {}", dir.display()))?;
    let db_path = dir.join("iai.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("打开数据库失败: {}", db_path.display()))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .context("初始化 schema_migrations 失败")?;

    // 记录基线迁移版本 0（幂等）。
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version) VALUES (0)",
        [],
    )
    .context("写入基线迁移记录失败")?;

    tracing::info!(path = %db_path.display(), "SQLite 已初始化");
    Ok(conn)
}
