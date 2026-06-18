//! 本地 SQLite 存储（rusqlite，bundled）。
//!
//! 阶段 1：在基线之上新增 `node`（本机节点身份，单行）与 `model_config`（已配置模型，
//! 含 Provider key）两张表，并提供节点初始化与模型仓储函数。
//!
//! 安全备注：阶段 1 为快速打通，`model_config.api_key` 以明文落库；密钥的安全存储
//! （keyring / 加密）列入 `DEVELOPMENT-PLAN.md` 阶段 7，对外 API 一律不回传 key。

use anyhow::Context;
use iai_node::{derive_capabilities, gen_node_id, NodeRole, NodeStatus, Provider};
use rusqlite::{params, Connection};
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

/// 打开（必要时创建）数据库连接，并确保迁移已应用。
pub fn open_conn() -> anyhow::Result<Connection> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("创建数据目录失败: {}", dir.display()))?;
    let db_path = dir.join("iai.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("打开数据库失败: {}", db_path.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    apply_migrations(&conn)?;
    Ok(conn)
}

/// 启动时初始化：打开连接、应用迁移、确保本机节点存在。
pub fn init_db() -> anyhow::Result<Connection> {
    let conn = open_conn()?;
    let node_id = ensure_node(&conn)?;
    tracing::info!(node_id = %node_id, path = %data_dir().join("iai.db").display(), "SQLite 已就绪");
    Ok(conn)
}

/// 幂等迁移：以 `schema_migrations` 记录已应用版本。
fn apply_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL DEFAULT (datetime('now'))
         );",
    )
    .context("初始化 schema_migrations 失败")?;
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version) VALUES (0)",
        [],
    )?;

    let applied: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))
        .unwrap_or(0);

    // v1：节点身份 + 模型配置。
    if applied < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS node (
                 node_id    TEXT PRIMARY KEY,
                 role       TEXT NOT NULL,
                 status     TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS model_config (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 provider   TEXT NOT NULL,
                 model      TEXT NOT NULL,
                 label      TEXT NOT NULL,
                 api_key    TEXT,
                 created_at TEXT NOT NULL DEFAULT (datetime('now')),
                 UNIQUE(provider, model)
             );",
        )
        .context("应用迁移 v1 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (1)", [])?;
    }

    Ok(())
}

/// 本机节点身份（持久化形态）。
pub struct StoredNode {
    pub node_id: String,
    pub role: NodeRole,
    pub status: NodeStatus,
}

/// 确保本机节点存在；首次调用生成并落库。返回 node_id。
pub fn ensure_node(conn: &Connection) -> anyhow::Result<String> {
    if let Some(n) = get_node(conn)? {
        return Ok(n.node_id);
    }
    let role = NodeRole::Captain;
    let node_id = gen_node_id(role);
    conn.execute(
        "INSERT INTO node (node_id, role, status) VALUES (?1, ?2, ?3)",
        params![node_id, role.as_str(), "available"],
    )
    .context("写入本机节点失败")?;
    Ok(node_id)
}

/// 读取本机节点（不存在返回 None）。
pub fn get_node(conn: &Connection) -> anyhow::Result<Option<StoredNode>> {
    let row = conn
        .query_row(
            "SELECT node_id, role, status FROM node LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .ok();
    Ok(row.map(|(node_id, role, status)| StoredNode {
        node_id,
        role: NodeRole::from_str(&role),
        status: match status.as_str() {
            "busy" => NodeStatus::Busy,
            "offline" => NodeStatus::Offline,
            _ => NodeStatus::Available,
        },
    }))
}

/// 已配置模型的对外展示形态（不含 key）。
pub struct StoredModel {
    pub provider: String,
    pub model: String,
    pub label: String,
}

/// 新增一个模型配置（provider+model 唯一，重复则覆盖 label/key）。
pub fn add_model(
    conn: &Connection,
    provider: &Provider,
    model: &str,
    api_key: Option<&str>,
) -> anyhow::Result<StoredModel> {
    let label = format!("{} · {}", provider.display(), model);
    conn.execute(
        "INSERT INTO model_config (provider, model, label, api_key)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(provider, model) DO UPDATE SET label=excluded.label, api_key=excluded.api_key",
        params![provider.id(), model, label, api_key],
    )
    .context("写入模型配置失败")?;
    Ok(StoredModel {
        provider: provider.id(),
        model: model.to_string(),
        label,
    })
}

/// 列出已配置模型（不含 key），按配置时间排序。
pub fn list_models(conn: &Connection) -> anyhow::Result<Vec<StoredModel>> {
    let mut stmt = conn.prepare(
        "SELECT provider, model, label FROM model_config ORDER BY created_at, id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(StoredModel {
                provider: r.get(0)?,
                model: r.get(1)?,
                label: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 能力声明（由已配置模型数推导）。
pub fn capabilities(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let count = list_models(conn)?.len();
    Ok(derive_capabilities(count))
}
