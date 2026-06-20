//! 本地 SQLite 存储（rusqlite，bundled）。
//!
//! 阶段 1：在基线之上新增 `node`（本机节点身份，单行）与 `model_config`（已配置模型，
//! 含 Provider key）两张表，并提供节点初始化与模型仓储函数。
//!
//! 安全备注：阶段 1 为快速打通，`model_config.api_key` 以明文落库；密钥的安全存储
//! （keyring / 加密）列入 `DEVELOPMENT-PLAN.md` 阶段 7，对外 API 一律不回传 key。

use anyhow::Context;
use iai_core::gen_task_id;
use iai_core::lifecycle::TaskState;
use iai_economic::ledger::{self, LedgerEntry, LedgerKind};
use iai_economic::market::{self, Ask};
use iai_node::registry::{NetworkStat, TeamMember};
use iai_node::{derive_capabilities, gen_node_id, NodeRole, NodeStatus, Provider};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::PathBuf;

/// 当前 UTC 秒（UTC+8 的展示在 economic::ledger::display_time 内完成）。
pub fn now_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 数据目录解析顺序（与 systemd 部署保持一致）：
/// 1. `$IAI_HOME`（显式优先）
/// 2. `/var/lib/iai` —— 仅当 `iai.db` 已存在（系统服务用），CLI 改密码时能命中同一份
/// 3. `$HOME/.iai` —— 开发/单机
/// 4. `.iai` —— 当前目录兜底
pub fn data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("IAI_HOME") {
        return PathBuf::from(home);
    }
    let systemd_default = PathBuf::from("/var/lib/iai");
    if systemd_default.join("iai.db").exists() {
        return systemd_default;
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

    // v2：哈希链 append-only 账本。seq 由应用层连续分配，entry_hash 唯一。
    if applied < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ledger (
                 seq          INTEGER PRIMARY KEY,
                 ts_epoch     INTEGER NOT NULL,
                 kind         TEXT NOT NULL,
                 node_id      TEXT NOT NULL,
                 amount       INTEGER NOT NULL,
                 locked_delta INTEGER NOT NULL DEFAULT 0,
                 note         TEXT NOT NULL DEFAULT '',
                 prev_hash    TEXT NOT NULL,
                 entry_hash   TEXT NOT NULL UNIQUE
             );",
        )
        .context("应用迁移 v2 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (2)", [])?;
    }

    // v3：市场挂卖簿 + 价格历史点。
    if applied < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS market_ask (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 px_cents   INTEGER NOT NULL,
                 qty        INTEGER NOT NULL,
                 node_id    TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS price_point (
                 id       INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts_epoch INTEGER NOT NULL,
                 px_cents INTEGER NOT NULL
             );",
        )
        .context("应用迁移 v3 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (3)", [])?;
    }

    // v4：团队与注册表。
    if applied < 4 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS team (
                 id           INTEGER PRIMARY KEY AUTOINCREMENT,
                 name         TEXT NOT NULL,
                 recruit_text TEXT NOT NULL DEFAULT '',
                 created_at   TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS team_member (
                 node_id    TEXT PRIMARY KEY,
                 role       TEXT NOT NULL,
                 model      TEXT NOT NULL,
                 online     INTEGER NOT NULL DEFAULT 1,
                 credits    INTEGER NOT NULL DEFAULT 0,
                 is_self    INTEGER NOT NULL DEFAULT 0,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .context("应用迁移 v4 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (4)", [])?;
    }

    // v5：任务与子任务。
    if applied < 5 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task (
                 task_id    TEXT PRIMARY KEY,
                 title      TEXT NOT NULL,
                 repo       TEXT NOT NULL,
                 prompt     TEXT NOT NULL,
                 state      TEXT NOT NULL,
                 result     TEXT,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS subtask (
                 subtask_id    TEXT PRIMARY KEY,
                 task_id       TEXT NOT NULL,
                 seq           INTEGER NOT NULL,
                 role          TEXT NOT NULL,
                 assigned_node TEXT,
                 status        TEXT NOT NULL DEFAULT 'wait',
                 attempts      INTEGER NOT NULL DEFAULT 0,
                 content       TEXT,
                 quality_score REAL,
                 created_at    TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .context("应用迁移 v5 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (5)", [])?;
    }

    // v6：控制台访问控制 —— session 表（存 token 哈希 + 过期时间）。
    // 密码哈希本身存到独立文件 `$IAI_HOME/console_auth.json`（与 DB 隔离，便于独立备份/清除）。
    if applied < 6 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS auth_sessions (
                 token_hash TEXT PRIMARY KEY,
                 expires_at INTEGER NOT NULL,
                 created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires
                 ON auth_sessions(expires_at);",
        )
        .context("应用迁移 v6 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (6)", [])?;
    }

    // v7：协作任务市场 V2 —— task 扩展 + 角色库/任务角色/招募槽/操作日志。
    if applied < 7 {
        // task 扩展列（逐条 ADD COLUMN；容忍重复以保幂等）。
        for col in [
            "repo_kind TEXT NOT NULL DEFAULT 'opensource'",
            "repo_url TEXT NOT NULL DEFAULT ''",
            "server_host TEXT NOT NULL DEFAULT ''",
            "server_path TEXT NOT NULL DEFAULT ''",
            "branch TEXT NOT NULL DEFAULT ''",
            "reward_total INTEGER NOT NULL DEFAULT 0",
            "reward_locked INTEGER NOT NULL DEFAULT 0",
            "captain_node TEXT NOT NULL DEFAULT ''",
            "visibility TEXT NOT NULL DEFAULT 'private'",
            "archived_at TEXT",
        ] {
            let _ = conn.execute(&format!("ALTER TABLE task ADD COLUMN {col}"), []);
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS role_template (
                 id           INTEGER PRIMARY KEY AUTOINCREMENT,
                 node_id      TEXT NOT NULL,
                 name         TEXT NOT NULL,
                 prompt       TEXT NOT NULL DEFAULT '',
                 is_captain   INTEGER NOT NULL DEFAULT 0,
                 model_filter TEXT NOT NULL DEFAULT 'any',
                 created_at   TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE TABLE IF NOT EXISTS task_role (
                 id            INTEGER PRIMARY KEY AUTOINCREMENT,
                 task_id       TEXT NOT NULL,
                 name          TEXT NOT NULL,
                 prompt        TEXT NOT NULL DEFAULT '',
                 recruit_count INTEGER NOT NULL DEFAULT 1,
                 model_filter  TEXT NOT NULL DEFAULT 'any',
                 is_captain    INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS assignment (
                 id            INTEGER PRIMARY KEY AUTOINCREMENT,
                 task_id       TEXT NOT NULL,
                 task_role_id  INTEGER NOT NULL,
                 slot_index    INTEGER NOT NULL,
                 node_id       TEXT,
                 model         TEXT,
                 worktree_path TEXT,
                 status        TEXT NOT NULL DEFAULT 'open',
                 attempts      INTEGER NOT NULL DEFAULT 0,
                 tokens        INTEGER NOT NULL DEFAULT 0,
                 started_at    TEXT,
                 ended_at      TEXT
             );
             CREATE TABLE IF NOT EXISTS op_log (
                 id      INTEGER PRIMARY KEY AUTOINCREMENT,
                 task_id TEXT NOT NULL,
                 ts      TEXT NOT NULL DEFAULT (datetime('now')),
                 actor   TEXT NOT NULL,
                 action  TEXT NOT NULL,
                 detail  TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_assignment_task ON assignment(task_id);
             CREATE INDEX IF NOT EXISTS idx_taskrole_task ON task_role(task_id);
             CREATE INDEX IF NOT EXISTS idx_oplog_task ON op_log(task_id);
             CREATE INDEX IF NOT EXISTS idx_roletmpl_node ON role_template(node_id);",
        )
        .context("应用迁移 v7 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (7)", [])?;
    }

    // v8：结算分配（贡献分 / 奖金平分记录）。
    if applied < 8 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reward_alloc (
                 id      INTEGER PRIMARY KEY AUTOINCREMENT,
                 task_id TEXT NOT NULL,
                 node_id TEXT NOT NULL,
                 role    TEXT NOT NULL,
                 credits INTEGER NOT NULL,
                 basis   TEXT,
                 ts      TEXT NOT NULL DEFAULT (datetime('now','+8 hours'))
             );
             CREATE INDEX IF NOT EXISTS idx_reward_alloc_task ON reward_alloc(task_id);",
        )
        .context("应用迁移 v8 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (8)", [])?;
    }

    // v9：模型工作态（单模型单任务约束 + token/工作时长，需求 8/9）。
    if applied < 9 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS model_instance (
                 node_id      TEXT NOT NULL,
                 model        TEXT NOT NULL,
                 status       TEXT NOT NULL DEFAULT 'idle',
                 current_task TEXT,
                 tokens_used  INTEGER NOT NULL DEFAULT 0,
                 work_seconds INTEGER NOT NULL DEFAULT 0,
                 updated_at   TEXT,
                 PRIMARY KEY (node_id, model)
             );",
        )
        .context("应用迁移 v9 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (9)", [])?;
    }

    // v10：通用键值设置（托管匹配开关等）。
    if applied < 10 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_setting (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .context("应用迁移 v10 失败")?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (10)", [])?;
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

/* ---------- 阶段 2：哈希链账本 ---------- */

const ENTRY_COLS: &str =
    "seq, ts_epoch, kind, node_id, amount, locked_delta, note, prev_hash, entry_hash";

fn map_entry(r: &Row) -> rusqlite::Result<LedgerEntry> {
    let kind_s: String = r.get(2)?;
    Ok(LedgerEntry {
        seq: r.get::<_, i64>(0)? as u64,
        ts_epoch: r.get(1)?,
        kind: LedgerKind::from_str(&kind_s).unwrap_or(LedgerKind::Settle),
        node_id: r.get(3)?,
        amount: r.get(4)?,
        locked_delta: r.get(5)?,
        note: r.get(6)?,
        prev_hash: r.get(7)?,
        entry_hash: r.get(8)?,
    })
}

/// 追加一条账本记录（事务内分配 seq、串接 prev_hash、计算 entry_hash）。
///
/// 阶段 3 市场买卖、阶段 5 任务锁定、阶段 6 结算分发都复用本函数，确保所有经济事件
/// 都进入同一条防篡改链。
fn next_seq_and_prev(conn: &Connection) -> anyhow::Result<(u64, String)> {
    let last: Option<(i64, String)> = conn
        .query_row(
            "SELECT seq, entry_hash FROM ledger ORDER BY seq DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match last {
        Some((s, h)) => (s as u64 + 1, h),
        None => (1, ledger::GENESIS_PREV.to_string()),
    })
}

/// 在当前连接/事务内插入一条账本记录（**不自管事务**）。
///
/// 既供 [`append_entry`] 包一层事务使用，也供市场撮合 [`execute_buy`] 在同一事务内
/// 落账 —— 保证「扣减挂单」与「买入记账」原子提交。
fn insert_entry(
    conn: &Connection,
    kind: LedgerKind,
    node_id: &str,
    amount: i64,
    locked_delta: i64,
    note: &str,
) -> anyhow::Result<LedgerEntry> {
    let (seq, prev) = next_seq_and_prev(conn)?;
    let ts = now_epoch();
    let entry_hash =
        ledger::compute_entry_hash(seq, ts, kind, node_id, amount, locked_delta, note, &prev);
    conn.execute(
        "INSERT INTO ledger (seq, ts_epoch, kind, node_id, amount, locked_delta, note, prev_hash, entry_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![seq as i64, ts, kind.as_str(), node_id, amount, locked_delta, note, prev, entry_hash],
    )
    .context("写入账本记录失败")?;
    Ok(LedgerEntry {
        seq,
        ts_epoch: ts,
        kind,
        node_id: node_id.to_string(),
        amount,
        locked_delta,
        note: note.to_string(),
        prev_hash: prev,
        entry_hash,
    })
}

/// 追加一条账本记录（自管事务）。
pub fn append_entry(
    conn: &Connection,
    kind: LedgerKind,
    node_id: &str,
    amount: i64,
    locked_delta: i64,
    note: &str,
) -> anyhow::Result<LedgerEntry> {
    let tx = conn.unchecked_transaction()?;
    let e = insert_entry(&tx, kind, node_id, amount, locked_delta, note)?;
    tx.commit()?;
    Ok(e)
}

/// 全量记录（seq 升序 —— 供链校验，跨所有节点）。
pub fn all_entries_asc(conn: &Connection) -> anyhow::Result<Vec<LedgerEntry>> {
    let sql = format!("SELECT {ENTRY_COLS} FROM ledger ORDER BY seq ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 本机视角账本（按 node_id 过滤，seq 升序）—— 钱包推导用。
///
/// 阶段 6 起，结算会把贡献点记到「成员节点」名下（去中心化下各节点各有账户），
/// 故本机钱包只汇总本机 node_id 的条目；链校验 [`all_entries_asc`] 仍覆盖全链。
pub fn entries_for(conn: &Connection, node: &str) -> anyhow::Result<Vec<LedgerEntry>> {
    let sql = format!("SELECT {ENTRY_COLS} FROM ledger WHERE node_id = ?1 ORDER BY seq ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![node], map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 本机视角流水（最新在前）。
pub fn list_ledger_desc_for(
    conn: &Connection,
    node: &str,
    limit: u32,
) -> anyhow::Result<Vec<LedgerEntry>> {
    let sql =
        format!("SELECT {ENTRY_COLS} FROM ledger WHERE node_id = ?1 ORDER BY seq DESC LIMIT ?2");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![node, limit as i64], map_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/* ---------- 阶段 3：市场 ---------- */

fn map_ask(r: &Row) -> rusqlite::Result<Ask> {
    Ok(Ask {
        id: r.get(0)?,
        px_cents: r.get(1)?,
        qty: r.get(2)?,
        node_id: r.get(3)?,
    })
}

/// 挂卖簿（价格升序，最低价在前 —— 即最优成交顺序）。
pub fn list_asks_asc(conn: &Connection) -> anyhow::Result<Vec<Ask>> {
    let mut stmt =
        conn.prepare("SELECT id, px_cents, qty, node_id FROM market_ask ORDER BY px_cents ASC, id ASC")?;
    let rows = stmt.query_map([], map_ask)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 当前最低挂单价（分）。空簿返回 None。
fn lowest_px(conn: &Connection) -> anyhow::Result<Option<i64>> {
    let v: Option<i64> = conn.query_row("SELECT MIN(px_cents) FROM market_ask", [], |r| r.get(0))?;
    Ok(v)
}

/// 记录一个价格历史点（当前最低挂单价）。空簿则跳过。
fn record_price_point(conn: &Connection) -> anyhow::Result<()> {
    if let Some(px) = lowest_px(conn)? {
        conn.execute(
            "INSERT INTO price_point (ts_epoch, px_cents) VALUES (?1, ?2)",
            params![now_epoch(), px],
        )?;
    }
    Ok(())
}

/// 挂出一个卖单，并记录价格点。
pub fn add_ask(conn: &Connection, px_cents: i64, qty: i64, node_id: &str) -> anyhow::Result<Ask> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO market_ask (px_cents, qty, node_id) VALUES (?1, ?2, ?3)",
        params![px_cents, qty, node_id],
    )?;
    let id = tx.last_insert_rowid();
    record_price_point(&tx)?;
    tx.commit()?;
    Ok(Ask { id, px_cents, qty, node_id: node_id.to_string() })
}

/// 买入成交结果。
pub struct BuyOutcome {
    pub filled: i64,
    pub cost_cents: i64,
}

/// 按最低价逐笔吃单：扣减挂单 + 买入落账 + 记录价格点，全程原子提交。
pub fn execute_buy(conn: &Connection, buyer_node: &str, need: i64) -> anyhow::Result<BuyOutcome> {
    let tx = conn.unchecked_transaction()?;
    let asks = list_asks_asc(&tx)?;
    let plan = market::plan_buy(&asks, need);
    for (id, take) in &plan.takes {
        tx.execute("UPDATE market_ask SET qty = qty - ?1 WHERE id = ?2", params![take, id])?;
    }
    tx.execute("DELETE FROM market_ask WHERE qty <= 0", [])?;
    if plan.filled > 0 {
        let note = format!("市场购入 {} 币 · ¥{:.2}", plan.filled, plan.cost_cents as f64 / 100.0);
        insert_entry(&tx, LedgerKind::Buy, buyer_node, plan.filled, 0, &note)?;
    }
    record_price_point(&tx)?;
    tx.commit()?;
    Ok(BuyOutcome { filled: plan.filled, cost_cents: plan.cost_cents })
}

/// 价格历史点（按时间升序，最近 `limit` 个），单位：分。
pub fn list_price_points(conn: &Connection, limit: u32) -> anyhow::Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT px_cents FROM (SELECT id, px_cents FROM price_point ORDER BY id DESC LIMIT ?1) ORDER BY id ASC",
    )?;
    let rows = stmt
        .query_map([limit as i64], |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/* ---------- 阶段 4：团队与注册表 ---------- */

/// 本机首个已配置模型的展示名（无则「未配置」）。
fn self_model(conn: &Connection) -> anyhow::Result<String> {
    Ok(list_models(conn)?
        .first()
        .map(|m| m.label.clone())
        .unwrap_or_else(|| "未配置".to_string()))
}

/// 确保本机作为「队长」成员登记在注册表中（幂等 upsert：刷新角色/模型/在线）。
pub fn ensure_self_member(conn: &Connection) -> anyhow::Result<()> {
    let node_id = ensure_node(conn)?;
    let role = get_node(conn)?.map(|n| n.role).unwrap_or(NodeRole::Captain);
    let model = self_model(conn)?;
    conn.execute(
        "INSERT INTO team_member (node_id, role, model, online, credits, is_self)
         VALUES (?1, ?2, ?3, 1, 0, 1)
         ON CONFLICT(node_id) DO UPDATE SET role=excluded.role, model=excluded.model, online=1, is_self=1",
        params![node_id, role.display_zh(), model],
    )?;
    Ok(())
}

/// 创建团队并发布招募（本机成为队长成员）。返回 team_id。
pub fn create_team(conn: &Connection, name: &str, recruit: &str) -> anyhow::Result<i64> {
    ensure_self_member(conn)?;
    conn.execute(
        "INSERT INTO team (name, recruit_text) VALUES (?1, ?2)",
        params![name, recruit],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 邀请 / 登记一个成员节点（幂等 upsert）。
pub fn invite_member(
    conn: &Connection,
    node_id: &str,
    role: &str,
    model: &str,
    credits: i64,
    online: bool,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO team_member (node_id, role, model, online, credits, is_self)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT(node_id) DO UPDATE SET role=excluded.role, model=excluded.model,
             online=excluded.online, credits=excluded.credits",
        params![node_id, role, model, online as i64, credits],
    )?;
    Ok(())
}

fn map_member(r: &Row) -> rusqlite::Result<TeamMember> {
    Ok(TeamMember {
        node_id: r.get(0)?,
        role: r.get(1)?,
        model: r.get(2)?,
        online: r.get::<_, i64>(3)? != 0,
        credits: r.get(4)?,
        is_self: r.get::<_, i64>(5)? != 0,
    })
}

/// 团队成员列表（本机在前，其余按累计贡献降序）。
pub fn list_team(conn: &Connection) -> anyhow::Result<Vec<TeamMember>> {
    ensure_self_member(conn)?;
    let mut stmt = conn.prepare(
        "SELECT node_id, role, model, online, credits, is_self FROM team_member
         ORDER BY is_self DESC, credits DESC, node_id ASC",
    )?;
    let rows = stmt.query_map([], map_member)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 网络概况（在线成员 / 已知节点 / 公开团队数）。
pub fn network_stat(conn: &Connection) -> anyhow::Result<NetworkStat> {
    ensure_self_member(conn)?;
    let members_online: i64 =
        conn.query_row("SELECT COUNT(*) FROM team_member WHERE online = 1", [], |r| r.get(0))?;
    let discovered: i64 = conn.query_row("SELECT COUNT(*) FROM team_member", [], |r| r.get(0))?;
    let public_teams: i64 = conn.query_row("SELECT COUNT(*) FROM team", [], |r| r.get(0))?;
    Ok(NetworkStat { members_online, discovered, public_teams })
}

/* ---------- 阶段 5：任务与子任务 ---------- */

/// 任务（持久化形态）。
pub struct TaskRow {
    pub task_id: String,
    pub title: String,
    pub repo: String,
    pub prompt: String,
    pub state: TaskState,
    pub result: Option<String>,
}

/// 子任务（持久化形态）。
pub struct SubtaskRow {
    pub subtask_id: String,
    pub role: String,
    pub assigned_node: Option<String>,
    pub status: String,
    pub attempts: i64,
}

/// 创建任务（初始状态由调用方给定）。返回 task_id。
pub fn create_task(
    conn: &Connection,
    title: &str,
    repo: &str,
    prompt: &str,
    state: TaskState,
) -> anyhow::Result<String> {
    let task_id = gen_task_id();
    conn.execute(
        "INSERT INTO task (task_id, title, repo, prompt, state) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, title, repo, prompt, state.as_str()],
    )?;
    Ok(task_id)
}

/// 添加一个子任务（初始状态 wait）。
pub fn add_subtask(
    conn: &Connection,
    task_id: &str,
    seq: i64,
    role: &str,
    assigned_node: Option<&str>,
) -> anyhow::Result<String> {
    let subtask_id = format!("{task_id}.{seq}");
    conn.execute(
        "INSERT INTO subtask (subtask_id, task_id, seq, role, assigned_node, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'wait')",
        params![subtask_id, task_id, seq, role, assigned_node],
    )?;
    Ok(subtask_id)
}

pub fn set_task_state(conn: &Connection, task_id: &str, state: TaskState) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE task SET state = ?1 WHERE task_id = ?2",
        params![state.as_str(), task_id],
    )?;
    Ok(())
}

pub fn set_task_result(conn: &Connection, task_id: &str, result: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE task SET result = ?1 WHERE task_id = ?2",
        params![result, task_id],
    )?;
    Ok(())
}

pub fn set_subtask_status(conn: &Connection, subtask_id: &str, status: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE subtask SET status = ?1 WHERE subtask_id = ?2",
        params![status, subtask_id],
    )?;
    Ok(())
}

/// 结束一个子任务：写状态/内容/裁判分/重试次数。
pub fn finish_subtask(
    conn: &Connection,
    subtask_id: &str,
    status: &str,
    content: &str,
    quality_score: f64,
    attempts: i64,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE subtask SET status = ?1, content = ?2, quality_score = ?3, attempts = ?4
         WHERE subtask_id = ?5",
        params![status, content, quality_score, attempts, subtask_id],
    )?;
    Ok(())
}

fn map_task(r: &Row) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        task_id: r.get(0)?,
        title: r.get(1)?,
        repo: r.get(2)?,
        prompt: r.get(3)?,
        state: TaskState::from_str(&r.get::<_, String>(4)?),
        result: r.get(5)?,
    })
}

pub fn get_task(conn: &Connection, task_id: &str) -> anyhow::Result<Option<TaskRow>> {
    let row = conn
        .query_row(
            "SELECT task_id, title, repo, prompt, state, result FROM task WHERE task_id = ?1",
            params![task_id],
            map_task,
        )
        .optional()?;
    Ok(row)
}

/// 任务列表（最新创建在前）。
pub fn list_tasks(conn: &Connection) -> anyhow::Result<Vec<TaskRow>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, title, repo, prompt, state, result FROM task ORDER BY created_at DESC, rowid DESC",
    )?;
    let rows = stmt.query_map([], map_task)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn map_subtask(r: &Row) -> rusqlite::Result<SubtaskRow> {
    Ok(SubtaskRow {
        subtask_id: r.get(0)?,
        role: r.get(1)?,
        assigned_node: r.get(2)?,
        status: r.get(3)?,
        attempts: r.get(4)?,
    })
}

/// 某任务的子任务（按 seq 升序）。
pub fn list_subtasks(conn: &Connection, task_id: &str) -> anyhow::Result<Vec<SubtaskRow>> {
    let mut stmt = conn.prepare(
        "SELECT subtask_id, role, assigned_node, status, attempts FROM subtask
         WHERE task_id = ?1 ORDER BY seq ASC",
    )?;
    let rows = stmt
        .query_map(params![task_id], map_subtask)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 结算结果。
pub struct SettleResult {
    pub total: i64,
    pub nodes: i64,
}

/// 结算闭环（`Aggregated → Settled`）：对每个通过质量门禁的 done 子任务，
/// 按 `120 × 裁判分` 向其执行节点分发贡献点 —— 写入哈希链账本（FR-010/011）并累加
/// 团队成员的累计贡献。挂单扣减/买入/结算共用同一条防篡改链。原子提交。
pub fn settle_task(conn: &Connection, task_id: &str, title: &str) -> anyhow::Result<SettleResult> {
    let self_id = ensure_node(conn)?;
    let dones: Vec<(String, Option<String>, f64)> = {
        let mut stmt = conn.prepare(
            "SELECT role, assigned_node, COALESCE(quality_score, 0.7) FROM subtask
             WHERE task_id = ?1 AND status = 'done' ORDER BY seq",
        )?;
        let rows = stmt
            .query_map(params![task_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, f64>(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let tx = conn.unchecked_transaction()?;
    let mut total = 0i64;
    let mut seen: Vec<String> = Vec::new();
    for (role, node, score) in dones {
        let node = node.unwrap_or_else(|| self_id.clone());
        let reward = (120.0 * score).round() as i64;
        let note = format!("任务「{title}」{role}提交被采纳");
        insert_entry(&tx, LedgerKind::Settle, &node, reward, 0, &note)?;
        tx.execute(
            "UPDATE team_member SET credits = credits + ?1 WHERE node_id = ?2",
            params![reward, node],
        )?;
        total += reward;
        if !seen.contains(&node) {
            seen.push(node);
        }
    }
    tx.commit()?;
    Ok(SettleResult { total, nodes: seen.len() as i64 })
}

/* ---------- 阶段 8：协作任务市场 V2 ---------- */

/// 队长角色默认驱动提示词（需求 3/11）。
pub const CAPTAIN_PROMPT_DEFAULT: &str = "你是队长节点：负责把任务目标拆解并派发给各角色，\
收集与审查各角色产出，对照目标不达标则退回对应角色重做，全部达标后汇总归档并分配贡献分。你本身不参与开发。";

/// 角色模板（节点角色库）。
pub struct RoleTemplate {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub is_captain: bool,
    pub model_filter: String,
}

/// 仓库规格（创建任务输入）。
pub struct RepoSpec {
    pub kind: String, // 'opensource' | 'internal'
    pub url: String,
    pub host: String,
    pub path: String,
    pub branch: String,
}

/// 任务开发角色规格（创建任务输入；队长由后端自动补）。
pub struct TaskRoleSpec {
    pub name: String,
    pub prompt: String,
    pub recruit_count: i64,
    pub model_filter: String,
}

/// 确保本机角色库存在内置「队长」模板（幂等，不可删）。
pub fn ensure_captain_role(conn: &Connection) -> anyhow::Result<()> {
    let node_id = ensure_node(conn)?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM role_template WHERE node_id = ?1 AND is_captain = 1",
        params![node_id],
        |r| r.get(0),
    )?;
    if n == 0 {
        conn.execute(
            "INSERT INTO role_template (node_id, name, prompt, is_captain, model_filter)
             VALUES (?1, '队长', ?2, 1, 'any')",
            params![node_id, CAPTAIN_PROMPT_DEFAULT],
        )?;
    }
    Ok(())
}

/// 本机角色库（队长在前）。
pub fn list_roles(conn: &Connection) -> anyhow::Result<Vec<RoleTemplate>> {
    ensure_captain_role(conn)?;
    let node_id = ensure_node(conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, name, prompt, is_captain, model_filter FROM role_template
         WHERE node_id = ?1 ORDER BY is_captain DESC, id ASC",
    )?;
    let rows = stmt
        .query_map(params![node_id], |r| {
            Ok(RoleTemplate {
                id: r.get(0)?,
                name: r.get(1)?,
                prompt: r.get(2)?,
                is_captain: r.get::<_, i64>(3)? != 0,
                model_filter: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 新增非队长角色。返回 id。
pub fn add_role(conn: &Connection, name: &str, prompt: &str, model_filter: &str) -> anyhow::Result<i64> {
    let node_id = ensure_node(conn)?;
    conn.execute(
        "INSERT INTO role_template (node_id, name, prompt, is_captain, model_filter)
         VALUES (?1, ?2, ?3, 0, ?4)",
        params![node_id, name, prompt, model_filter],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 更新角色（队长仅允许改 prompt）。
pub fn update_role(
    conn: &Connection,
    id: i64,
    name: Option<&str>,
    prompt: Option<&str>,
    model_filter: Option<&str>,
) -> anyhow::Result<()> {
    let is_captain: i64 = conn
        .query_row("SELECT is_captain FROM role_template WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?
        .unwrap_or(0);
    if let Some(p) = prompt {
        conn.execute("UPDATE role_template SET prompt = ?1 WHERE id = ?2", params![p, id])?;
    }
    if is_captain == 0 {
        if let Some(nm) = name {
            conn.execute("UPDATE role_template SET name = ?1 WHERE id = ?2", params![nm, id])?;
        }
        if let Some(m) = model_filter {
            conn.execute("UPDATE role_template SET model_filter = ?1 WHERE id = ?2", params![m, id])?;
        }
    }
    Ok(())
}

/// 删除角色。队长不可删（返回 false）。
pub fn delete_role(conn: &Connection, id: i64) -> anyhow::Result<bool> {
    let is_captain: i64 = conn
        .query_row("SELECT is_captain FROM role_template WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?
        .unwrap_or(0);
    if is_captain == 1 {
        return Ok(false);
    }
    conn.execute("DELETE FROM role_template WHERE id = ?1", params![id])?;
    Ok(true)
}

/// 追加一条任务操作日志（需求 12，UTC+8 时间）。
pub fn append_op_log(
    conn: &Connection,
    task_id: &str,
    actor: &str,
    action: &str,
    detail: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO op_log (task_id, ts, actor, action, detail)
         VALUES (?1, datetime('now','+8 hours'), ?2, ?3, ?4)",
        params![task_id, actor, action, detail],
    )?;
    Ok(())
}

/// 任务操作日志（时间升序）。
pub fn list_op_log(conn: &Connection, task_id: &str) -> anyhow::Result<Vec<(String, String, String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT ts, actor, action, detail FROM op_log WHERE task_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt
        .query_map(params![task_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 创建 V2 任务：扩展列 + 队长角色 + 开发角色及招募槽 + 操作日志。返回 task_id。
/// 奖励金锁定（账本 Lock）由调用方在校验余额后处理。
pub fn create_task_v2(
    conn: &Connection,
    title: &str,
    repo: &RepoSpec,
    dev_roles: &[TaskRoleSpec],
    reward: i64,
    visibility: &str,
) -> anyhow::Result<String> {
    ensure_captain_role(conn)?;
    let captain_node = ensure_node(conn)?;
    let captain_prompt: String = conn
        .query_row(
            "SELECT prompt FROM role_template WHERE node_id = ?1 AND is_captain = 1 LIMIT 1",
            params![captain_node],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| CAPTAIN_PROMPT_DEFAULT.to_string());

    let task_id = gen_task_id();
    let branch = if repo.branch.trim().is_empty() {
        format!("task/{task_id}")
    } else {
        repo.branch.clone()
    };
    let repo_display = if repo.kind == "internal" {
        format!("{}:{}", repo.host, repo.path)
    } else {
        repo.url.clone()
    };

    conn.execute(
        "INSERT INTO task
           (task_id, title, repo, prompt, state, repo_kind, repo_url, server_host, server_path,
            branch, reward_total, reward_locked, captain_node, visibility)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            task_id, title, repo_display, title, "created",
            repo.kind, repo.url, repo.host, repo.path, branch,
            reward, reward, captain_node, visibility
        ],
    )?;

    // 队长角色（不生成招募槽）
    conn.execute(
        "INSERT INTO task_role (task_id, name, prompt, recruit_count, model_filter, is_captain)
         VALUES (?1, '队长', ?2, 1, 'any', 1)",
        params![task_id, captain_prompt],
    )?;
    // 开发角色 + 招募槽
    for spec in dev_roles {
        conn.execute(
            "INSERT INTO task_role (task_id, name, prompt, recruit_count, model_filter, is_captain)
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![task_id, spec.name, spec.prompt, spec.recruit_count.max(1), spec.model_filter],
        )?;
        let role_id = conn.last_insert_rowid();
        for slot in 0..spec.recruit_count.max(1) {
            conn.execute(
                "INSERT INTO assignment (task_id, task_role_id, slot_index, status)
                 VALUES (?1, ?2, ?3, 'open')",
                params![task_id, role_id, slot],
            )?;
        }
    }

    append_op_log(
        conn,
        &task_id,
        &captain_node,
        "create",
        Some(&format!("repo={repo_display} branch={branch} reward={reward} dev_roles={}", dev_roles.len())),
    )?;
    Ok(task_id)
}

/* ---------- 阶段 9：执行流转 + 结算分配 ---------- */

/// 招募槽（含角色名）。
pub struct AssignmentRow {
    pub id: i64,
    pub role_name: String,
    pub slot_index: i64,
    pub node_id: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub tokens: i64,
}

/// 某任务的招募槽（按角色/槽序）。
pub fn list_assignments(conn: &Connection, task_id: &str) -> anyhow::Result<Vec<AssignmentRow>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, tr.name, a.slot_index, a.node_id, a.model, a.status, a.tokens
         FROM assignment a JOIN task_role tr ON a.task_role_id = tr.id
         WHERE a.task_id = ?1 ORDER BY a.task_role_id, a.slot_index",
    )?;
    let rows = stmt
        .query_map(params![task_id], |r| {
            Ok(AssignmentRow {
                id: r.get(0)?,
                role_name: r.get(1)?,
                slot_index: r.get(2)?,
                node_id: r.get(3)?,
                model: r.get(4)?,
                status: r.get(5)?,
                tokens: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 领取槽位（open → claimed）。
pub fn claim_assignment(conn: &Connection, id: i64, node: &str, model: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assignment SET node_id = ?1, model = ?2, status = 'claimed',
             started_at = datetime('now','+8 hours') WHERE id = ?3",
        params![node, model, id],
    )?;
    Ok(())
}

pub fn set_assignment_status(conn: &Connection, id: i64, status: &str) -> anyhow::Result<()> {
    conn.execute("UPDATE assignment SET status = ?1 WHERE id = ?2", params![status, id])?;
    Ok(())
}

/// 完成槽位（→ done，记录 token）。
pub fn finish_assignment(conn: &Connection, id: i64, tokens: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assignment SET status = 'done', tokens = ?1, ended_at = datetime('now','+8 hours') WHERE id = ?2",
        params![tokens, id],
    )?;
    Ok(())
}

/// 直接更新任务状态字符串（V2 扩展状态：recruiting/executing/reviewing/aggregated/settled）。
pub fn set_task_state_str(conn: &Connection, task_id: &str, state: &str) -> anyhow::Result<()> {
    conn.execute("UPDATE task SET state = ?1 WHERE task_id = ?2", params![state, task_id])?;
    Ok(())
}

/// 结算分配记录。
pub fn list_reward_alloc(
    conn: &Connection,
    task_id: &str,
) -> anyhow::Result<Vec<(String, String, i64, Option<String>)>> {
    let mut stmt = conn
        .prepare("SELECT node_id, role, credits, basis FROM reward_alloc WHERE task_id = ?1 ORDER BY id")?;
    let rows = stmt
        .query_map(params![task_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// V2 结算结果。
pub struct SettleV2 {
    pub total: i64,
    pub nodes: i64,
    pub bonus: bool, // true=保底（未配奖金）
}

/// 结算闭环（needs 11/5）：done 槽位按节点分配贡献分。
/// - 配奖金：在 done 节点间**均分** reward_total（队长 Unlock 释放锁定，各节点 Settle 入账）；
/// - 未配：每个 done 节点**保底 +1**。
/// 同时累加 team_member 贡献、写 reward_alloc、置 settled+归档、记 op_log。原子提交。
pub fn settle_task_v2(conn: &Connection, task_id: &str) -> anyhow::Result<SettleV2> {
    let (captain, reward_total, title): (String, i64, String) = conn.query_row(
        "SELECT captain_node, reward_total, title FROM task WHERE task_id = ?1",
        params![task_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // done 槽位的 (node, role)，按节点去重保序
    let dones: Vec<(String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT a.node_id, tr.name FROM assignment a JOIN task_role tr ON a.task_role_id = tr.id
             WHERE a.task_id = ?1 AND a.status = 'done' AND a.node_id IS NOT NULL ORDER BY a.id",
        )?;
        let rows = stmt
            .query_map(params![task_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut nodes: Vec<(String, String)> = Vec::new();
    for (n, role) in dones {
        if !nodes.iter().any(|(x, _)| x == &n) {
            nodes.push((n, role));
        }
    }

    let tx = conn.unchecked_transaction()?;
    let bonus = reward_total <= 0;
    let mut total = 0i64;

    if !bonus {
        // 释放队长锁定（钱已分给贡献者，仅清锁定池）
        insert_entry(&tx, LedgerKind::Unlock, &captain, 0, -reward_total, &format!("任务「{title}」结算释放锁定"))?;
        let cnt = nodes.len().max(1) as i64;
        let per = reward_total / cnt;
        let rem = reward_total % cnt;
        for (i, (node, role)) in nodes.iter().enumerate() {
            let share = per + if (i as i64) < rem { 1 } else { 0 };
            insert_entry(&tx, LedgerKind::Settle, node, share, 0, &format!("任务「{title}」奖金分配"))?;
            tx.execute("UPDATE team_member SET credits = credits + ?1 WHERE node_id = ?2", params![share, node])?;
            tx.execute(
                "INSERT INTO reward_alloc (task_id, node_id, role, credits, basis) VALUES (?1, ?2, ?3, ?4, '奖金均分')",
                params![task_id, node, role, share],
            )?;
            total += share;
        }
    } else {
        for (node, role) in &nodes {
            insert_entry(&tx, LedgerKind::Settle, node, 1, 0, &format!("任务「{title}」保底贡献"))?;
            tx.execute("UPDATE team_member SET credits = credits + 1 WHERE node_id = ?1", params![node])?;
            tx.execute(
                "INSERT INTO reward_alloc (task_id, node_id, role, credits, basis) VALUES (?1, ?2, ?3, 1, '保底')",
                params![task_id, node, role],
            )?;
            total += 1;
        }
    }

    tx.execute(
        "UPDATE task SET state = 'settled', archived_at = datetime('now','+8 hours') WHERE task_id = ?1",
        params![task_id],
    )?;
    tx.commit()?;

    let basis = if bonus { "保底" } else { "奖金均分" };
    append_op_log(conn, task_id, &captain, "settle", Some(&format!("{basis} 共 {total} 币 → {} 节点", nodes.len())))?;
    append_op_log(conn, task_id, &captain, "archive", Some("任务已归档"))?;
    Ok(SettleV2 { total, nodes: nodes.len() as i64, bonus })
}

/* ---------- 阶段 10a：模型工作态（单模型单任务 + token/时长，需求 8/9） ---------- */

/// 模型工作态（每 (node, model) 一行）。
pub struct ModelInstanceRow {
    pub node_id: String,
    pub model: String,
    pub status: String,
    pub current_task: Option<String>,
    pub tokens_used: i64,
    pub work_seconds: i64,
}

/// 标记某 (node, model) 进入工作（busy + 当前任务）。
pub fn set_model_busy(conn: &Connection, node: &str, model: &str, task_id: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO model_instance (node_id, model, status, current_task, updated_at)
         VALUES (?1, ?2, 'busy', ?3, datetime('now','+8 hours'))
         ON CONFLICT(node_id, model) DO UPDATE SET status='busy', current_task=?3, updated_at=datetime('now','+8 hours')",
        params![node, model, task_id],
    )?;
    Ok(())
}

/// 标记某 (node, model) 空闲，并累加本次 token / 工作秒数。
pub fn set_model_idle(conn: &Connection, node: &str, model: &str, tokens: i64, seconds: i64) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO model_instance (node_id, model, status, current_task, tokens_used, work_seconds, updated_at)
         VALUES (?1, ?2, 'idle', NULL, ?3, ?4, datetime('now','+8 hours'))
         ON CONFLICT(node_id, model) DO UPDATE SET status='idle', current_task=NULL,
             tokens_used = tokens_used + ?3, work_seconds = work_seconds + ?4, updated_at=datetime('now','+8 hours')",
        params![node, model, tokens, seconds],
    )?;
    Ok(())
}

/// 单模型单任务约束：某 (node, model) 是否正忙。
pub fn is_model_busy(conn: &Connection, node: &str, model: &str) -> anyhow::Result<bool> {
    let s: Option<String> = conn
        .query_row(
            "SELECT status FROM model_instance WHERE node_id = ?1 AND model = ?2",
            params![node, model],
            |r| r.get(0),
        )
        .optional()?;
    Ok(s.as_deref() == Some("busy"))
}

/// 所有模型工作态（本机视角的网络模型，需求 9 队长查看全部）。
pub fn list_model_instances(conn: &Connection) -> anyhow::Result<Vec<ModelInstanceRow>> {
    let mut stmt = conn.prepare(
        "SELECT node_id, model, status, current_task, tokens_used, work_seconds
         FROM model_instance ORDER BY node_id, model",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ModelInstanceRow {
                node_id: r.get(0)?,
                model: r.get(1)?,
                status: r.get(2)?,
                current_task: r.get(3)?,
                tokens_used: r.get(4)?,
                work_seconds: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/* ---------- 阶段 10b-2：通用设置（托管开关等） ---------- */

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=?2",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM app_setting WHERE key = ?1", params![key], |r| r.get(0))
        .optional()?;
    Ok(v)
}

pub fn is_hosted(conn: &Connection) -> anyhow::Result<bool> {
    Ok(get_setting(conn, "hosted")?.as_deref() == Some("1"))
}

pub fn set_hosted(conn: &Connection, enabled: bool) -> anyhow::Result<()> {
    set_setting(conn, "hosted", if enabled { "1" } else { "0" })
}

/* ---------- 阶段 11：真实执行（Provider + worktree） ---------- */

/// 本机首个模型配置（含 key，仅内部执行用，不经 API）。返回 (provider, model, key)。
pub fn first_model_with_key(conn: &Connection) -> anyhow::Result<Option<(String, String, Option<String>)>> {
    let row = conn
        .query_row(
            "SELECT provider, model, api_key FROM model_config ORDER BY created_at, id LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)),
        )
        .optional()?;
    Ok(row)
}

/// 任务仓库信息：(kind, url, host, path, branch)。
pub fn get_task_repo(conn: &Connection, task_id: &str) -> anyhow::Result<Option<(String, String, String, String, String)>> {
    let row = conn
        .query_row(
            "SELECT repo_kind, repo_url, server_host, server_path, branch FROM task WHERE task_id = ?1",
            params![task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    Ok(row)
}

/// 记录 assignment 的 worktree 路径。
pub fn set_assignment_worktree(conn: &Connection, id: i64, path: &str) -> anyhow::Result<()> {
    conn.execute("UPDATE assignment SET worktree_path = ?1 WHERE id = ?2", params![path, id])?;
    Ok(())
}

/// 踢出后槽位回市场（status open，清空领取信息，attempts+1）。
pub fn reopen_assignment(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE assignment SET status='open', node_id=NULL, model=NULL, started_at=NULL, attempts=attempts+1 WHERE id=?1",
        params![id],
    )?;
    Ok(())
}

/// 超时的 working 槽（started_at 早于 now-seconds，UTC+8 口径）。返回 (id, task_id, node, role)。
pub fn list_stale_working(
    conn: &Connection,
    seconds: i64,
) -> anyhow::Result<Vec<(i64, String, Option<String>, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.task_id, a.node_id, tr.name FROM assignment a JOIN task_role tr ON a.task_role_id = tr.id
         WHERE a.status='working' AND a.started_at IS NOT NULL
           AND (strftime('%s','now') + 28800 - strftime('%s', a.started_at)) > ?1",
    )?;
    let rows = stmt
        .query_map(params![seconds], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
