//! 本地 HTTP API（axum）。
//!
//! 阶段 0：`/api/health`、`/api/version` + 内嵌前端回退。
//! 阶段 1：`/api/node`（本机节点状态）、`/api/node/models`（GET 列出 / POST 新增）。
//! 后续阶段在此挂载 `/api/wallet`、`/api/ledger`、`/api/market/*`、`/api/team`、`/api/tasks/*`。

use axum::{
    http::StatusCode,
    routing::get,
    Json, Router,
};
use iai_economic::{credit, ledger};
use iai_node::Provider;
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
