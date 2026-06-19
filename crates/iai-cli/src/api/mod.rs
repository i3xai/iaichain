//! 本地 HTTP API（axum）。
//!
//! 阶段 0：`/api/health`、`/api/version` + 内嵌前端回退。
//! 阶段 1：`/api/node`（本机节点状态）、`/api/node/models`（GET 列出 / POST 新增）。
//! 后续阶段在此挂载 `/api/wallet`、`/api/ledger`、`/api/market/*`、`/api/team`、`/api/tasks/*`。

use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use iai_economic::{credit, ledger, market};
use iai_node::{registry, Provider};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{embed::static_handler, storage};

/// 启动本地服务，仅绑定回环地址。
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let _conn = storage::init_db()?;

    let app = router();
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("IAI Chain 节点已启动");
    tracing::info!("  落地页:  http://{addr}/");
    tracing::info!("  控制台:  http://{addr}/console");
    tracing::info!("  健康检查: http://{addr}/api/health");

    axum::serve(listener, app).await?;
    Ok(())
}

/// 组装路由：先匹配 `/api/*`，未命中则回退到内嵌静态资源。
pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/node", get(node))
        .route("/api/node/models", get(list_models).post(add_model))
        .route("/api/wallet", get(wallet))
        .route("/api/ledger", get(ledger_list))
        .route("/api/market/book", get(market_book))
        .route("/api/market/price", get(market_price))
        .route("/api/market/buy", post(market_buy))
        .route("/api/market/sell", post(market_sell))
        .route("/api/team", get(team))
        .route("/api/team/recruit", post(team_recruit))
        .route("/api/team/invite", post(team_invite))
        .route("/api/network", get(network))
        .fallback(static_handler)
}

/// 统一错误映射：领域/存储错误 → 500 + JSON。
type ApiResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn err500(e: anyhow::Error) -> (StatusCode, Json<Value>) {
    tracing::error!(error = %e, "API 处理失败");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
}

/* ---------- 阶段 0 ---------- */

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "node": "iai-chain" }))
}

async fn version() -> Json<Value> {
    Json(json!({ "name": "iai-chain", "version": env!("CARGO_PKG_VERSION") }))
}

/* ---------- 阶段 1：节点身份与模型 ---------- */

/// GET /api/node —— 本机节点状态。
async fn node() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    storage::ensure_node(&conn).map_err(err500)?;
    let n = storage::get_node(&conn).map_err(err500)?.expect("节点已确保存在");
    let models = storage::list_models(&conn).map_err(err500)?;
    let labels: Vec<String> = models.iter().map(|m| m.label.clone()).collect();
    let caps = storage::capabilities(&conn).map_err(err500)?;

    Ok(Json(json!({
        "id": n.node_id,
        "role": n.role.display_zh(),
        "online": n.status.is_online(),
        // 负载实时监控属阶段 5（任务编排），此处为占位 0。
        "load": 0,
        "models": labels,
        "capabilities": caps,
        "modelConfigured": !labels.is_empty(),
    })))
}

/// GET /api/node/models —— 已配置模型列表（不含 key）。
async fn list_models() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let models = storage::list_models(&conn).map_err(err500)?;
    let items: Vec<Value> = models
        .iter()
        .map(|m| json!({ "provider": m.provider, "model": m.model, "label": m.label }))
        .collect();
    Ok(Json(json!({ "models": items })))
}

#[derive(Deserialize)]
struct AddModelReq {
    provider: String,
    model: Option<String>,
    key: Option<String>,
}

/// POST /api/node/models —— 新增模型配置。
async fn add_model(Json(req): Json<AddModelReq>) -> ApiResult {
    let provider = Provider::parse(&req.provider);
    let model = req
        .model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| provider.default_model().to_string());

    if provider.requires_key() && req.key.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{} 需要 --key", provider.display()) })),
        ));
    }

    let conn = storage::open_conn().map_err(err500)?;
    let saved = storage::add_model(&conn, &provider, &model, req.key.as_deref()).map_err(err500)?;
    Ok(Json(json!({
        "ok": true,
        "model": { "provider": saved.provider, "model": saved.model, "label": saved.label }
    })))
}

/* ---------- 阶段 2：钱包与账本 ---------- */

/// GET /api/wallet —— 由账本推导的钱包视图。
async fn wallet() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let entries = storage::all_entries_asc(&conn).map_err(err500)?;
    let w = credit::derive_wallet(&entries, storage::now_epoch());
    Ok(Json(json!({
        "balance": w.balance,
        "locked": w.locked,
        "weekly": w.weekly,
        "lockedTasks": w.locked_tasks,
        "weeklyAccepted": w.weekly_accepted,
    })))
}

/// GET /api/ledger —— 最近账本流水（最新在前），直接返回数组。
async fn ledger_list() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let entries = storage::list_ledger_desc(&conn, 50).map_err(err500)?;
    let items: Vec<Value> = entries
        .iter()
        .map(|e| {
            let delta = if e.amount >= 0 {
                format!("+{}", e.amount)
            } else {
                e.amount.to_string()
            };
            json!({
                "time": ledger::display_time(e.ts_epoch),
                "type": e.kind.display_zh(),
                "note": e.note,
                "delta": delta,
            })
        })
        .collect();
    Ok(Json(Value::Array(items)))
}

/* ---------- 阶段 3：市场 ---------- */

fn ask_json(px_cents: i64, qty: i64, node: &str) -> Value {
    json!({ "px": market::yuan(px_cents), "qty": qty, "node": node })
}

/// GET /api/market/book —— 挂卖簿（价格升序），直接返回数组。
async fn market_book() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let asks = storage::list_asks_asc(&conn).map_err(err500)?;
    let items: Vec<Value> = asks.iter().map(|a| ask_json(a.px_cents, a.qty, &a.node_id)).collect();
    Ok(Json(Value::Array(items)))
}

/// GET /api/market/price —— 价格历史点 [{ i, px }]（时间升序）。
async fn market_price() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let pts = storage::list_price_points(&conn, 64).map_err(err500)?;
    let items: Vec<Value> = pts
        .iter()
        .enumerate()
        .map(|(i, px)| json!({ "i": i, "px": market::yuan(*px) }))
        .collect();
    Ok(Json(Value::Array(items)))
}

#[derive(Deserialize)]
struct BuyReq {
    qty: i64,
}

/// POST /api/market/buy —— 本机按最低价买入；返回新簿 + 成交量 + 成交额。
async fn market_buy(Json(req): Json<BuyReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let node = storage::ensure_node(&conn).map_err(err500)?;
    let out = storage::execute_buy(&conn, &node, req.qty).map_err(err500)?;
    let asks = storage::list_asks_asc(&conn).map_err(err500)?;
    let orders: Vec<Value> = asks.iter().map(|a| ask_json(a.px_cents, a.qty, &a.node_id)).collect();
    Ok(Json(json!({
        "orders": orders,
        "filled": out.filled,
        "cost": market::yuan(out.cost_cents),
    })))
}

#[derive(Deserialize)]
struct SellReq {
    px: f64,
    qty: i64,
    node: Option<String>,
}

/// POST /api/market/sell —— 挂出卖单。
async fn market_sell(Json(req): Json<SellReq>) -> ApiResult {
    if req.qty <= 0 || req.px <= 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "px 与 qty 必须为正" })),
        ));
    }
    let conn = storage::open_conn().map_err(err500)?;
    let node = match req.node {
        Some(n) if !n.trim().is_empty() => n,
        _ => storage::ensure_node(&conn).map_err(err500)?,
    };
    let px_cents = market::cents_from_yuan(req.px);
    let ask = storage::add_ask(&conn, px_cents, req.qty, &node).map_err(err500)?;
    Ok(Json(json!({ "ok": true, "ask": ask_json(ask.px_cents, ask.qty, &ask.node_id) })))
}

/* ---------- 阶段 4：团队与网络 ---------- */

/// GET /api/team —— 团队成员，直接返回 [[name, role, model, online01, creditsStr]]。
async fn team() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let members = storage::list_team(&conn).map_err(err500)?;
    let rows: Vec<Value> = members
        .iter()
        .map(|m| {
            let name = if m.is_self {
                format!("本机 · {}", m.node_id)
            } else {
                m.node_id.clone()
            };
            let credits = if m.is_self {
                "—".to_string()
            } else {
                registry::format_credits(m.credits)
            };
            json!([name, m.role, m.model, i32::from(m.online), credits])
        })
        .collect();
    Ok(Json(Value::Array(rows)))
}

/// GET /api/network —— 网络概况。
async fn network() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let s = storage::network_stat(&conn).map_err(err500)?;
    Ok(Json(json!({
        "membersOnline": s.members_online,
        "discovered": s.discovered,
        "publicTeams": s.public_teams,
    })))
}

#[derive(Deserialize)]
struct RecruitReq {
    name: Option<String>,
    recruit: String,
}

/// POST /api/team/recruit —— 创建团队并发布招募。
async fn team_recruit(Json(req): Json<RecruitReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let name = req.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "我的团队".to_string());
    let id = storage::create_team(&conn, &name, &req.recruit).map_err(err500)?;
    Ok(Json(json!({ "ok": true, "teamId": id })))
}

#[derive(Deserialize)]
struct InviteReq {
    node: String,
    role: String,
    model: String,
    credits: Option<i64>,
    online: Option<bool>,
}

/// POST /api/team/invite —— 邀请 / 登记成员节点。
async fn team_invite(Json(req): Json<InviteReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    storage::invite_member(
        &conn,
        &req.node,
        &req.role,
        &req.model,
        req.credits.unwrap_or(0),
        req.online.unwrap_or(true),
    )
    .map_err(err500)?;
    Ok(Json(json!({ "ok": true })))
}
