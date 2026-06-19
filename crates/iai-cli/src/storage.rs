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
