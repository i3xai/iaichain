//! 本地 HTTP API（axum）。
//!
//! 阶段 0：`/api/health`、`/api/version` + 内嵌前端回退。
//! 阶段 1：`/api/node`（本机节点状态）、`/api/node/models`（GET 列出 / POST 新增）。
//! 后续阶段在此挂载 `/api/wallet`、`/api/ledger`、`/api/market/*`、`/api/team`、`/api/tasks/*`。

use axum::{
    extract::{Path, Request},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use iai_economic::{credit, ledger, market};
use iai_node::{registry, Provider};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{auth, embed::static_handler, orchestrator, relay, storage};

/// 启动本地服务，仅绑定回环地址。
pub async fn serve(port: u16) -> anyhow::Result<()> {
    let _conn = storage::init_db()?;

    // 首次启动自动生成强随机密码（不会再出现 password_not_set 状态）。
    if let Some(plain) = auth::ensure_password_on_first_run()? {
        tracing::warn!(
            plain = %plain,
            file = %auth::initial_password_file().display(),
            "已自动生成控制台随机密码；明文已写入一次性文件（0600），请妥善保存后删除。"
        );
    }

    let app = router();
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("IAI Chain 节点已启动");
    tracing::info!("  落地页:  http://{addr}/");
    tracing::info!("  控制台:  http://{addr}/console");
    tracing::info!("  健康检查: http://{addr}/api/health");
    tracing::info!(
        "  改密码: `iai password set`；忘密码: `iai password reset`；查看初始密码: `iai password show`"
    );

    // 托管匹配后台循环（开启后空闲自动领取网络任务）
    tokio::spawn(hosted_loop());
    // 心跳监管：超时的 working 槽踢出重新招募（需求 10）
    tokio::spawn(watchdog_loop());
    axum::serve(listener, app).await?;
    Ok(())
}

/// 托管循环：每 8s 检查托管开关，开启且配了中继则自动匹配一次。
async fn hosted_loop() {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        let hosted = storage::open_conn().ok().and_then(|c| storage::is_hosted(&c).ok()).unwrap_or(false);
        if hosted && relay::relay_url().is_some() {
            if let Ok(Some(slot)) = auto_match_once().await {
                tracing::info!(slot = %slot, "托管自动领取槽位");
            }
        }
    }
}

/// 心跳监管循环：每 30s 检测超时（>120s 未完成）的 working 槽 → 踢出 + 回市场重新招募。
async fn watchdog_loop() {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        let conn = match storage::open_conn() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok(stale) = storage::list_stale_working(&conn, 120) {
            for (id, task_id, _node, role) in stale {
                let _ = storage::reopen_assignment(&conn, id);
                let _ = storage::append_op_log(
                    &conn,
                    &task_id,
                    "watchdog",
                    "kick",
                    Some(&format!("「{role}」超时无响应（重试 3 次）· 踢出重新招募")),
                );
                tracing::info!(task = %task_id, role = %role, "超时踢出重新招募");
            }
        }
    }
}

/// 组装路由：
/// - 公开：`/api/health`、`/api/version`、`/api/auth/*`
/// - 受保护：其它所有 `/api/*`（需 Authorization: Bearer <token>）
/// - 静态资源：落地页 `/`、控制台 `/console` 不保护（前端自己处理登录）
pub fn router() -> Router {
    // 受保护的 API 路由子树。
    let protected = Router::new()
        .route("/api/node", get(node).put(node_set_role))
        .route("/api/node/models", get(list_models).post(add_model))
        .route("/api/wallet", get(wallet))
        .route("/api/ledger", get(ledger_list))
        .route("/api/market/book", get(market_book))
        .route("/api/market/price", get(market_price))
        .route("/api/market/buy", post(market_buy))
        .route("/api/market/sell", post(market_sell))
        .route("/api/team", get(team))
        .route("/api/team/idle", get(team_idle))
        .route("/api/team/recruit", post(team_recruit))
        .route("/api/team/invite", post(team_invite))
        .route("/api/team/join", post(team_join))
        .route("/api/team/join-requests", get(team_join_requests))
        .route("/api/team/join-requests/decide", post(team_join_decide))
        .route("/api/network", get(network))
        .route("/api/tasks", get(tasks_list).post(tasks_create))
        .route("/api/tasks/:id", get(task_detail))
        .route("/api/tasks/:id/log", get(task_log))
        .route("/api/tasks/:id/start", post(task_start))
        .route("/api/tasks/:id/recruit/apply", post(recruit_apply))
        .route("/api/tasks/:id/recruit/applications", get(recruit_list))
        .route("/api/tasks/:id/recruit/decide", post(recruit_decide))
        .route("/api/tasks/compose", post(tasks_compose))
        .route("/api/roles", get(roles_list).post(roles_add))
        .route("/api/roles/:id", put(roles_update).delete(roles_delete))
        .route("/api/repo/check", post(repo_check))
        .route("/api/models/instances", get(models_instances))
        .route("/api/network/tasks", get(network_tasks))
        .route("/api/network/claim", post(network_claim))
        .route("/api/match/auto", post(match_auto))
        .route("/api/match/hosted", get(match_hosted_get).put(match_hosted))
        .route("/api/auth/logout", post(auth_logout))
        .layer(middleware::from_fn(require_auth));

    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/login", post(auth_login))
        .merge(protected)
        .fallback(static_handler)
}

/// 鉴权中间件：从 Authorization 头提取 Bearer token，校验通过则放行，
/// 否则返回 401 + `{ error: "..." }`。
///
/// 启动期已保证密码一定存在（首次启动自动生成），所以这里不再处理 password_not_set 分支。
async fn require_auth(req: Request, next: Next) -> Result<Response, Response> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());

    let Some(token) = token else {
        return Err(unauthorized_response("missing_token", "缺少 Authorization: Bearer <token>"));
    };

    let conn = match storage::open_conn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "鉴权时打开 DB 失败");
            return Err(internal_error_response(&e));
        }
    };

    if !auth::validate_session(&conn, &token) {
        return Err(unauthorized_response("invalid_token", "token 无效或已过期，请重新登录"));
    }

    Ok(next.run(req).await)
}

fn unauthorized_response(code: &str, message: &str) -> Response {
    use axum::response::IntoResponse;
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": code, "message": message })),
    )
        .into_response()
}

fn internal_error_response(e: &anyhow::Error) -> Response {
    use axum::response::IntoResponse;
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal", "message": e.to_string() })),
    )
        .into_response()
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
        "roleKey": n.role.as_str(),
        "isCaptain": n.role == iai_node::NodeRole::Captain,
        "online": n.status.is_online(),
        // 负载实时监控属阶段 5（任务编排），此处为占位 0。
        "load": 0,
        "models": labels,
        "capabilities": caps,
        "modelConfigured": !labels.is_empty(),
    })))
}

#[derive(Deserialize)]
struct NodeRoleReq {
    /// captain | member
    role: String,
}

/// PUT /api/node —— 设置本机节点角色（队长 / 队员）。
async fn node_set_role(Json(req): Json<NodeRoleReq>) -> ApiResult {
    let role = match req.role.trim().to_ascii_lowercase().as_str() {
        "captain" | "队长" => iai_node::NodeRole::Captain,
        "member" | "队员" => iai_node::NodeRole::Member,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "role 须为 captain 或 member" })),
            ))
        }
    };
    let conn = storage::open_conn().map_err(err500)?;
    storage::set_node_role(&conn, role).map_err(err500)?;
    Ok(Json(json!({ "ok": true, "role": role.display_zh(), "roleKey": role.as_str() })))
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
    let self_id = storage::ensure_node(&conn).map_err(err500)?;
    let entries = storage::entries_for(&conn, &self_id).map_err(err500)?;
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
    let self_id = storage::ensure_node(&conn).map_err(err500)?;
    let entries = storage::list_ledger_desc_for(&conn, &self_id, 50).map_err(err500)?;
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

/// POST /api/team/invite —— 邀请 / 登记成员节点（仅队长）。
async fn team_invite(Json(req): Json<InviteReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可邀请队员" }))));
    }
    let captain = storage::ensure_node(&conn).map_err(err500)?;
    storage::invite_member(
        &conn,
        &req.node,
        &req.role,
        &req.model,
        req.credits.unwrap_or(0),
        req.online.unwrap_or(true),
    )
    .map_err(err500)?;
    let _ = storage::mark_join_approved(&conn, &captain, &req.node, &req.role, &req.model);
    // 同步中继批准态，便于跨节点领取鉴权（FR-107）
    if let Some(url) = relay::relay_url() {
        let _ = relay::join_decide_remote(
            &url,
            &captain,
            &req.node,
            true,
            Some(&req.role),
            Some(&req.model),
            None,
        )
        .await;
    }
    Ok(Json(json!({ "ok": true })))
}

/// GET /api/team/idle —— 在线空闲队员（供队长邀请）。
async fn team_idle() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可查看空闲队员" }))));
    }
    let members = storage::list_idle_members(&conn).map_err(err500)?;
    let rows: Vec<Value> = members
        .iter()
        .map(|m| {
            json!({
                "nodeId": m.node_id,
                "role": m.role,
                "model": m.model,
                "online": m.online,
                "credits": m.credits,
                "idle": true,
            })
        })
        .collect();
    Ok(Json(json!({ "members": rows })))
}

#[derive(Deserialize)]
struct JoinReq {
    /// 队长节点 id（申请人填写）
    #[serde(rename = "captainNodeId")]
    captain_node_id: String,
    role: Option<String>,
    model: Option<String>,
    /// 加入词 / 留言
    message: Option<String>,
}

/// POST /api/team/join —— 申请加入某队长团队（写入中继 + 本地镜像）。
async fn team_join(Json(req): Json<JoinReq>) -> ApiResult {
    let captain = req.captain_node_id.trim();
    if captain.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "缺少 captainNodeId" }))));
    }
    let conn = storage::open_conn().map_err(err500)?;
    let applicant = storage::ensure_node(&conn).map_err(err500)?;
    if applicant == captain {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "不能申请加入自己的团队" }))));
    }
    let role = req.role.unwrap_or_else(|| "开发".into());
    let model = req
        .model
        .or_else(|| storage::self_model(&conn).ok())
        .unwrap_or_default();
    let message = req.message.unwrap_or_default();
    storage::upsert_join_request(&conn, captain, &applicant, &role, &model, &message)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    let url = relay::relay_url().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "未配置中继 IAI_RELAY，无法跨节点申请入队" })),
        )
    })?;
    relay::join_apply_remote(&url, captain, &applicant, &role, &model, &message)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true, "status": "pending" })))
}

/// GET /api/team/join-requests —— 队长查看申请（中继优先，并镜像本地）。
async fn team_join_requests() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可审批入队申请" }))));
    }
    let captain = storage::ensure_node(&conn).map_err(err500)?;
    // 以本地为准（含留言/拒绝原因）；中继仅补充 pending 镜像
    if let Some(url) = relay::relay_url() {
        if let Ok(remote) = relay::fetch_joins(&url, Some(&captain)).await {
            for j in remote {
                if j.status == "pending" {
                    let _ = storage::upsert_join_request(
                        &conn, &j.captain, &j.applicant, &j.role, &j.model, "",
                    );
                }
            }
        }
    }
    let mut items: Vec<Value> = Vec::new();
    for r in storage::list_join_requests(&conn, &captain).map_err(err500)? {
        items.push(json!({
            "captainNodeId": r.captain_node,
            "applicantNodeId": r.applicant_node,
            "role": r.role,
            "model": r.model,
            "status": r.status,
            "message": r.message,
            "rejectReason": r.reject_reason,
            "createdAt": r.created_at,
        }));
    }
    Ok(Json(json!({ "requests": items })))
}

#[derive(Deserialize)]
struct JoinDecideBody {
    #[serde(rename = "applicantNodeId")]
    applicant_node_id: String,
    approve: bool,
    role: Option<String>,
    model: Option<String>,
    #[serde(rename = "rejectReason")]
    reject_reason: Option<String>,
}

/// POST /api/team/join-requests/decide —— 队长批准/拒绝。
async fn team_join_decide(Json(req): Json<JoinDecideBody>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可审批" }))));
    }
    let captain = storage::ensure_node(&conn).map_err(err500)?;
    let applicant = req.applicant_node_id.trim();
    if applicant.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "缺少 applicantNodeId" }))));
    }
    if !req.approve && req.reject_reason.as_deref().unwrap_or("").trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "拒绝时须填写原因" }))));
    }
    // 若本地无记录，先从中继拉一条镜像
    if let Some(url) = relay::relay_url() {
        if let Ok(remote) = relay::fetch_joins(&url, Some(&captain)).await {
            if let Some(j) = remote.iter().find(|j| j.applicant == applicant) {
                let _ = storage::upsert_join_request(&conn, &captain, applicant, &j.role, &j.model, "");
            }
        }
    }
    storage::decide_join_request(
        &conn,
        &captain,
        applicant,
        req.approve,
        req.role.as_deref(),
        req.model.as_deref(),
        req.reject_reason.as_deref(),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    if let Some(url) = relay::relay_url() {
        relay::join_decide_remote(
            &url,
            &captain,
            applicant,
            req.approve,
            req.role.as_deref(),
            req.model.as_deref(),
            req.reject_reason.as_deref(),
        )
        .await
        .map_err(err500)?;
    }
    Ok(Json(json!({ "ok": true, "approve": req.approve })))
}

/* ---------- 阶段 5：任务编排 ---------- */

/// 把任务 + 子任务组装为前端任务卡所需结构 { id, t, repo, st, pct, roles, stateKey }。
fn task_json(conn: &Connection, t: &storage::TaskRow) -> anyhow::Result<Value> {
    let state_label = match t.state_key.as_str() {
        "recruiting" => "招募中",
        "ready_to_run" => "招募完成可运行",
        "executing" | "working" => "执行中",
        "reviewing" => "评审中",
        "aggregated" => "已采纳",
        "settled" => "已结算",
        other if other == t.state.as_str() => t.state.display_zh(),
        other => other,
    };
    // V2 任务（compose 创建）：用招募槽 assignment 计算角色与进度。
    let assigns = storage::list_assignments(conn, &t.task_id)?;
    if !assigns.is_empty() {
        let total = assigns.len();
        let done = assigns.iter().filter(|a| a.status == "done").count();
        let settled = t.state.is_delivered() || (total > 0 && done == total);
        let pct = if settled {
            100
        } else if t.state_key == "ready_to_run" {
            0
        } else if total > 0 {
            done * 100 / total
        } else {
            0
        };
        let st = if settled {
            "done"
        } else if t.state_key == "recruiting" {
            "recruiting"
        } else if t.state_key == "ready_to_run" {
            "ready"
        } else {
            "run"
        };
        let roles: Vec<Value> = assigns
            .iter()
            .map(|a| {
                let s = match a.status.as_str() {
                    "done" => "done",
                    "working" | "claimed" => "run",
                    _ => "wait",
                };
                json!([a.role_name, s])
            })
            .collect();
        return Ok(json!({
            "id": t.task_id, "t": t.title, "repo": t.repo, "st": st, "pct": pct,
            "roles": roles, "stateKey": t.state_key, "stateLabel": state_label
        }));
    }
    let subs = storage::list_subtasks(conn, &t.task_id)?;
    let total = subs.len().max(1);
    let done = subs.iter().filter(|s| s.status == "done").count();
    let pct = if t.state.is_delivered() { 100 } else { done * 100 / total };
    let st = if t.state.is_delivered() { "done" } else { "run" };
    let roles: Vec<Value> = subs
        .iter()
        .map(|s| {
            let status = match s.status.as_str() {
                "done" => "done",
                "run" => "run",
                _ => "wait",
            };
            json!([s.role, status])
        })
        .collect();
    Ok(json!({
        "id": t.task_id, "t": t.title, "repo": t.repo, "st": st, "pct": pct,
        "roles": roles, "stateKey": t.state_key, "stateLabel": state_label
    }))
}

/// GET /api/tasks —— 任务列表（最新在前）。
async fn tasks_list() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let tasks = storage::list_tasks(&conn).map_err(err500)?;
    let mut arr = Vec::with_capacity(tasks.len());
    for t in &tasks {
        arr.push(task_json(&conn, t).map_err(err500)?);
    }
    Ok(Json(Value::Array(arr)))
}

#[derive(Deserialize)]
struct CreateTaskReq {
    prompt: String,
    repo: String,
}

/// POST /api/tasks —— 发起任务：解析→分解→匹配（同步），随后异步驱动执行。
async fn tasks_create(Json(req): Json<CreateTaskReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可发起任务" }))));
    }
    let task_id = orchestrator::create_task(&conn, &req.prompt, &req.repo)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    let id = task_id.clone();
    tokio::spawn(async move {
        if let Err(e) = orchestrator::drive(id).await {
            tracing::error!(error = %e, "任务驱动失败");
        }
    });
    Ok(Json(json!({ "ok": true, "taskId": task_id })))
}

/// GET /api/tasks/:id —— 任务详情（含状态/聚合结果）。
async fn task_detail(Path(id): Path<String>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let Some(t) = storage::get_task(&conn, &id).map_err(err500)? else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "任务不存在" }))));
    };
    let mut v = task_json(&conn, &t).map_err(err500)?;
    if let Value::Object(ref mut m) = v {
        m.insert("state".into(), json!(match t.state_key.as_str() {
            "recruiting" => "招募中",
            "ready_to_run" => "招募完成可运行",
            "executing" => "执行中",
            "reviewing" => "评审中",
            _ => t.state.display_zh(),
        }));
        m.insert("stateKey".into(), json!(t.state_key));
        m.insert("result".into(), json!(t.result));
        let assigns = storage::list_assignments(&conn, &id).map_err(err500)?;
        m.insert(
            "assignments".into(),
            json!(assigns
                .iter()
                .map(|a| json!({ "role": a.role_name, "slot": a.slot_index, "node": a.node_id, "model": a.model, "status": a.status, "tokens": a.tokens }))
                .collect::<Vec<_>>()),
        );
        let rewards = storage::list_reward_alloc(&conn, &id).map_err(err500)?;
        m.insert(
            "rewards".into(),
            json!(rewards
                .iter()
                .map(|(node, role, credits, basis)| json!({ "node": node, "role": role, "credits": credits, "basis": basis }))
                .collect::<Vec<_>>()),
        );
        // 招募申请（队长可见）
        if storage::is_captain_node(&conn).map_err(err500)? {
            let apps = storage::list_recruit_applications(&conn, Some(&id)).map_err(err500)?;
            m.insert(
                "recruitApplications".into(),
                json!(apps.iter().map(|a| json!({
                    "id": a.id, "role": a.role_name, "applicant": a.applicant_node,
                    "message": a.message, "model": a.model, "status": a.status,
                    "rejectReason": a.reject_reason, "createdAt": a.created_at
                })).collect::<Vec<_>>()),
            );
        }
    }
    Ok(Json(v))
}

/* ---------- 阶段 8：角色库 / 仓库检测 / V2 任务创建 / 操作日志 ---------- */

async fn roles_list() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let roles = storage::list_roles(&conn).map_err(err500)?;
    let items: Vec<Value> = roles
        .iter()
        .map(|r| json!({ "id": r.id, "name": r.name, "prompt": r.prompt, "isCaptain": r.is_captain, "modelFilter": r.model_filter }))
        .collect();
    Ok(Json(Value::Array(items)))
}

#[derive(Deserialize)]
struct RoleAddReq {
    name: String,
    prompt: Option<String>,
    model_filter: Option<String>,
}

async fn roles_add(Json(req): Json<RoleAddReq>) -> ApiResult {
    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "角色名不能为空" }))));
    }
    let conn = storage::open_conn().map_err(err500)?;
    let id = storage::add_role(
        &conn,
        req.name.trim(),
        req.prompt.as_deref().unwrap_or(""),
        req.model_filter.as_deref().unwrap_or("any"),
    )
    .map_err(err500)?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
struct RoleUpdateReq {
    name: Option<String>,
    prompt: Option<String>,
    model_filter: Option<String>,
}

async fn roles_update(Path(id): Path<i64>, Json(req): Json<RoleUpdateReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    storage::update_role(&conn, id, req.name.as_deref(), req.prompt.as_deref(), req.model_filter.as_deref())
        .map_err(err500)?;
    Ok(Json(json!({ "ok": true })))
}

async fn roles_delete(Path(id): Path<i64>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let ok = storage::delete_role(&conn, id).map_err(err500)?;
    if !ok {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "队长角色不可删除" }))));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RepoCheckReq {
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    path: String,
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 连通性检测：开源用 git ls-remote；内部用 ssh + git（节点本机 ssh 免密前提）。
async fn check_repo(req: &RepoCheckReq) -> Result<Vec<String>, String> {
    use std::time::Duration;
    use tokio::process::Command;
    let dur = Duration::from_secs(8);
    let output = match req.kind.as_str() {
        "opensource" => {
            if req.url.trim().is_empty() {
                return Err("缺少仓库地址".into());
            }
            let fut = Command::new("git").args(["ls-remote", "--heads", req.url.trim()]).output();
            tokio::time::timeout(dur, fut).await
        }
        "internal" => {
            if req.host.trim().is_empty() || req.path.trim().is_empty() {
                return Err("缺少服务器地址或代码目录".into());
            }
            let p = shell_quote(req.path.trim());
            let remote = format!(
                "git -C {p} rev-parse --is-inside-work-tree >/dev/null 2>&1 && git -C {p} for-each-ref --format='%(refname:short)' refs/heads"
            );
            let fut = Command::new("ssh")
                .args([
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "BatchMode=yes",
                    "-o", "ConnectTimeout=6",
                    req.host.trim(),
                    &remote,
                ])
                .output();
            tokio::time::timeout(dur, fut).await
        }
        _ => return Err("未知仓库类型".into()),
    };
    let output = output
        .map_err(|_| "连接超时".to_string())?
        .map_err(|e| format!("执行失败: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("无法连通：{}", err.lines().next().unwrap_or("未知错误（非 git 仓库或不可达）")));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<String> = if req.kind == "opensource" {
        stdout
            .lines()
            .filter_map(|l| l.split("refs/heads/").nth(1).map(|s| s.trim().to_string()))
            .collect()
    } else {
        stdout.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };
    Ok(branches)
}

async fn repo_check(Json(req): Json<RepoCheckReq>) -> ApiResult {
    match check_repo(&req).await {
        Ok(branches) => Ok(Json(json!({ "ok": true, "branches": branches }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e }))),
    }
}

fn default_one() -> i64 {
    1
}
fn default_any() -> String {
    "any".into()
}
fn default_private() -> String {
    "private".into()
}

#[derive(Deserialize)]
struct ComposeRepo {
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    branch: String,
}

#[derive(Deserialize)]
struct ComposeRole {
    name: String,
    #[serde(default)]
    prompt: String,
    #[serde(default = "default_one")]
    recruit_count: i64,
    #[serde(default = "default_any")]
    model_filter: String,
}

#[derive(Deserialize)]
struct ComposeReq {
    title: String,
    repo: ComposeRepo,
    roles: Vec<ComposeRole>,
    #[serde(default)]
    reward: i64,
    #[serde(default = "default_private")]
    visibility: String,
}

/// POST /api/tasks/compose —— V2 任务创建（仓库+多角色+招募+奖金）。仅队长。
async fn tasks_compose(Json(req): Json<ComposeReq>) -> ApiResult {
    if req.title.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "任务标题不能为空" }))));
    }
    if req.roles.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "至少配置一个开发角色" }))));
    }
    if req.reward < 0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "奖励金不能为负" }))));
    }
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可发起任务" }))));
    }
    let self_id = storage::ensure_node(&conn).map_err(err500)?;

    // 奖金校验：不超过本机可用余额
    if req.reward > 0 {
        let entries = storage::entries_for(&conn, &self_id).map_err(err500)?;
        let w = credit::derive_wallet(&entries, storage::now_epoch());
        if req.reward > w.balance {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("奖励金 {} 超过可用余额 {}", req.reward, w.balance) })),
            ));
        }
    }

    let repo = storage::RepoSpec {
        kind: req.repo.kind.clone(),
        url: req.repo.url.clone(),
        host: req.repo.host.clone(),
        path: req.repo.path.clone(),
        branch: req.repo.branch.clone(),
    };
    let dev_roles: Vec<storage::TaskRoleSpec> = req
        .roles
        .iter()
        .map(|r| storage::TaskRoleSpec {
            name: r.name.clone(),
            prompt: r.prompt.clone(),
            recruit_count: r.recruit_count.max(1),
            model_filter: r.model_filter.clone(),
        })
        .collect();

    let task_id = storage::create_task_v2(&conn, req.title.trim(), &repo, &dev_roles, req.reward, &req.visibility)
        .map_err(err500)?;

    // 锁定奖励金（账本 Lock）
    if req.reward > 0 {
        storage::append_entry(
            &conn,
            ledger::LedgerKind::Lock,
            &self_id,
            -req.reward,
            req.reward,
            &format!("任务「{}」奖励金锁定", req.title.trim()),
        )
        .map_err(err500)?;
        storage::append_op_log(&conn, &task_id, &self_id, "lock", Some(&format!("锁定奖励金 {}", req.reward)))
            .map_err(err500)?;
    }

    // network + 配了中继 → 发布到公告板等招募；不自动执行，等招募满后队长 start
    let mut published = false;
    if req.visibility == "network" {
        if let Some(url) = relay::relay_url() {
            let mut slots = Vec::new();
            for role in &req.roles {
                for i in 0..role.recruit_count.max(1) {
                    slots.push(relay::SlotAd {
                        slot_id: format!("{task_id}#{}#{i}", role.name),
                        role: role.name.clone(),
                        model_filter: role.model_filter.clone(),
                        claimed_by: None,
                    });
                }
            }
            let repo_disp = if req.repo.kind == "internal" {
                format!("{}:{}", req.repo.host, req.repo.path)
            } else {
                req.repo.url.clone()
            };
            let ad = relay::TaskAd {
                task_id: task_id.clone(),
                title: req.title.trim().to_string(),
                repo: repo_disp,
                reward: req.reward,
                publisher: self_id.clone(),
                slots,
            };
            if relay::publish_task(&url, &ad).await.is_ok() {
                published = true;
            }
        }
    }
    let _ = storage::set_task_state_str(&conn, &task_id, "recruiting");
    let _ = storage::append_op_log(
        &conn,
        &task_id,
        &self_id,
        "recruiting",
        Some(if published {
            "已发布到网络公告板，等待队员申请角色"
        } else {
            "本地招募中，等待队员申请角色"
        }),
    );
    Ok(Json(json!({ "ok": true, "taskId": task_id, "published": published, "state": "recruiting" })))
}

#[derive(Deserialize)]
struct RecruitApplyBody {
    role: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    model: Option<String>,
    /// 网络任务发布者（跨节点申请时必填）
    #[serde(default)]
    publisher: Option<String>,
}

/// POST /api/tasks/:id/recruit/apply —— 队员申请任务角色（需队长审批）。
async fn recruit_apply(Path(id): Path<String>, Json(req): Json<RecruitApplyBody>) -> ApiResult {
    let role = req.role.trim();
    if role.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "请选择角色" }))));
    }
    let conn = storage::open_conn().map_err(err500)?;
    let applicant = storage::ensure_node(&conn).map_err(err500)?;
    let model = req
        .model
        .clone()
        .or_else(|| storage::self_model(&conn).ok())
        .unwrap_or_default();
    let message = req.message.trim();

    // 本机任务：直接写入本地申请表
    if storage::get_task(&conn, &id).map_err(err500)?.is_some() {
        storage::apply_recruit(&conn, &id, role, &applicant, message, &model)
            .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
        let _ = storage::append_op_log(
            &conn,
            &id,
            &applicant,
            "recruit_apply",
            Some(&format!("角色={role} 留言={message}")),
        );
        return Ok(Json(json!({ "ok": true, "status": "pending" })));
    }

    // 跨节点：走中继
    let url = relay::relay_url().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "未配置中继 IAI_RELAY，无法申请远程任务" })),
        )
    })?;
    let publisher = if let Some(p) = req.publisher.filter(|s| !s.trim().is_empty()) {
        p
    } else {
        // 从公告板推断
        let board = relay::fetch_board(&url).await.map_err(err500)?;
        board
            .into_iter()
            .find(|t| t.task_id == id)
            .map(|t| t.publisher)
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "任务不存在" })),
                )
            })?
    };
    relay::recruit_apply_remote(&url, &id, &publisher, role, &applicant, message, &model)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    Ok(Json(json!({ "ok": true, "status": "pending" })))
}

/// GET /api/tasks/:id/recruit/applications —— 队长查看招募申请。
async fn recruit_list(Path(id): Path<String>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可查看招募申请" }))));
    }
    let self_id = storage::ensure_node(&conn).map_err(err500)?;
    // 从中继镜像到本地
    if let Some(url) = relay::relay_url() {
        if let Ok(remote) = relay::fetch_recruits(&url, Some(&self_id), Some(&id)).await {
            for r in remote {
                if r.status == "pending" {
                    let _ = storage::apply_recruit(
                        &conn,
                        &r.task_id,
                        &r.role,
                        &r.applicant,
                        &r.message,
                        &r.model,
                    );
                }
            }
        }
    }
    let apps = storage::list_recruit_applications(&conn, Some(&id)).map_err(err500)?;
    let items: Vec<Value> = apps
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "taskId": a.task_id,
                "role": a.role_name,
                "applicant": a.applicant_node,
                "message": a.message,
                "model": a.model,
                "status": a.status,
                "rejectReason": a.reject_reason,
                "createdAt": a.created_at,
            })
        })
        .collect();
    Ok(Json(json!({ "applications": items })))
}

#[derive(Deserialize)]
struct RecruitDecideBody {
    role: String,
    #[serde(rename = "applicantNodeId")]
    applicant_node_id: String,
    approve: bool,
    #[serde(rename = "rejectReason")]
    reject_reason: Option<String>,
}

/// POST /api/tasks/:id/recruit/decide —— 队长批准/拒绝角色申请。
async fn recruit_decide(Path(id): Path<String>, Json(req): Json<RecruitDecideBody>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可审批招募" }))));
    }
    let role = req.role.trim();
    let applicant = req.applicant_node_id.trim();
    if role.is_empty() || applicant.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "缺少 role 或 applicantNodeId" }))));
    }
    if !req.approve && req.reject_reason.as_deref().unwrap_or("").trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "拒绝时须填写原因" }))));
    }
    let self_id = storage::ensure_node(&conn).map_err(err500)?;
    // 确保本地有申请记录（可能来自中继）
    if let Some(url) = relay::relay_url() {
        if let Ok(remote) = relay::fetch_recruits(&url, Some(&self_id), Some(&id)).await {
            if let Some(r) = remote.iter().find(|r| r.role == role && r.applicant == applicant) {
                let _ = storage::apply_recruit(&conn, &id, role, applicant, &r.message, &r.model);
            }
        }
    }
    storage::decide_recruit(
        &conn,
        &id,
        role,
        applicant,
        req.approve,
        req.reject_reason.as_deref(),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e.to_string() }))))?;
    if let Some(url) = relay::relay_url() {
        let _ = relay::recruit_decide_remote(
            &url,
            &id,
            role,
            applicant,
            req.approve,
            req.reject_reason.as_deref(),
        )
        .await;
    }
    let ready = storage::maybe_mark_ready_to_run(&conn, &id).map_err(err500)?;
    let _ = storage::append_op_log(
        &conn,
        &id,
        &self_id,
        if req.approve { "recruit_approve" } else { "recruit_reject" },
        Some(&format!(
            "角色={role} 申请人={applicant}{}",
            if req.approve {
                String::new()
            } else {
                format!(" 原因={}", req.reject_reason.as_deref().unwrap_or(""))
            }
        )),
    );
    Ok(Json(json!({ "ok": true, "approve": req.approve, "readyToRun": ready })))
}

/// POST /api/tasks/:id/start —— 招募完成后队长启动任务执行。
async fn task_start(Path(id): Path<String>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    if !storage::is_captain_node(&conn).map_err(err500)? {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "仅队长可启动任务" }))));
    }
    let Some(t) = storage::get_task(&conn, &id).map_err(err500)? else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "任务不存在" }))));
    };
    if t.state_key != "ready_to_run" {
        // 若槽已满但状态未刷，尝试标记
        if storage::recruitment_complete(&conn, &id).map_err(err500)? {
            storage::set_task_state_str(&conn, &id, "ready_to_run").map_err(err500)?;
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "招募尚未完成，无法启动" })),
            ));
        }
    }
    let self_id = storage::ensure_node(&conn).map_err(err500)?;
    storage::set_task_state_str(&conn, &id, "executing").map_err(err500)?;
    storage::append_op_log(&conn, &id, &self_id, "start", Some("队长启动任务执行"))
        .map_err(err500)?;
    let tid = id.clone();
    tokio::spawn(orchestrator::drive_v2(tid));
    Ok(Json(json!({ "ok": true, "state": "executing" })))
}

/// GET /api/tasks/:id/log —— 任务操作日志（需求 12）。
async fn task_log(Path(id): Path<String>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let logs = storage::list_op_log(&conn, &id).map_err(err500)?;
    let items: Vec<Value> = logs
        .iter()
        .map(|(ts, actor, action, detail)| json!({ "ts": ts, "actor": actor, "action": action, "detail": detail }))
        .collect();
    Ok(Json(Value::Array(items)))
}

/// GET /api/models/instances —— 模型工作态（需求 8/9）。
async fn models_instances() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let rows = storage::list_model_instances(&conn).map_err(err500)?;
    let items: Vec<Value> = rows
        .iter()
        .map(|m| json!({ "node": m.node_id, "model": m.model, "status": m.status, "currentTask": m.current_task, "tokensUsed": m.tokens_used, "workSeconds": m.work_seconds }))
        .collect();
    Ok(Json(Value::Array(items)))
}

/* ---------- 阶段 10b：节点网络（中继公告板领取） ---------- */

/// GET /api/network/tasks —— 网络可领取任务（从中继拉，过滤掉本节点发布的与已领满的）。
async fn network_tasks() -> ApiResult {
    let url = match relay::relay_url() {
        Some(u) => u,
        None => return Ok(Json(json!({ "relay": false, "tasks": [] }))),
    };
    let self_id = {
        let c = storage::open_conn().map_err(err500)?;
        storage::ensure_node(&c).map_err(err500)?
    };
    let board = relay::fetch_board(&url).await.map_err(err500)?;
    let tasks: Vec<Value> = board
        .iter()
        .filter(|t| t.publisher != self_id)
        .filter_map(|t| {
            let open: Vec<Value> = t
                .slots
                .iter()
                .filter(|s| s.claimed_by.is_none())
                .map(|s| json!({ "slotId": s.slot_id, "role": s.role, "modelFilter": s.model_filter }))
                .collect();
            if open.is_empty() {
                return None;
            }
            Some(json!({
                "taskId": t.task_id, "title": t.title, "repo": t.repo,
                "reward": t.reward, "publisher": t.publisher, "openSlots": open
            }))
        })
        .collect();
    Ok(Json(json!({ "relay": true, "tasks": tasks })))
}

#[derive(Deserialize)]
struct NetClaimReq {
    slot_id: String,
}

/// POST /api/network/claim —— 本节点领取网络任务槽（经中继原子占位）。
async fn network_claim(Json(req): Json<NetClaimReq>) -> ApiResult {
    let url = match relay::relay_url() {
        Some(u) => u,
        None => return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "未配置中继 IAI_RELAY" })))),
    };
    let self_id = {
        let c = storage::open_conn().map_err(err500)?;
        storage::ensure_node(&c).map_err(err500)?
    };
    // FR-107：未批准成员不得领取（发布者本人除外）
    let publisher = relay::publisher_of_slot(&url, &req.slot_id)
        .await
        .map_err(err500)?;
    let Some(publisher) = publisher else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "槽位不存在" }))));
    };
    if publisher != self_id {
        let ok = relay::is_approved_member(&url, &publisher, &self_id)
            .await
            .map_err(err500)?;
        if !ok {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "未加入该队长团队或申请尚未批准，无法领取" })),
            ));
        }
    }
    let ok = relay::claim_slot(&url, &req.slot_id, &self_id).await.map_err(err500)?;
    if ok {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err((StatusCode::CONFLICT, Json(json!({ "error": "槽位已被领取或不存在" }))))
    }
}

/// 自动匹配一次：在公告板里挑**奖金最高**、本机有空闲匹配模型的开放槽并领取。
/// 返回领到的 slot_id（无可匹配返回 None）。供手动「一键匹配」与「托管」共用。
pub async fn auto_match_once() -> anyhow::Result<Option<String>> {
    let url = match relay::relay_url() {
        Some(u) => u,
        None => return Ok(None),
    };
    let board = relay::fetch_board(&url).await?;
    let conn = storage::open_conn()?;
    let self_id = storage::ensure_node(&conn)?;
    let models = storage::list_models(&conn)?;

    let mut best: Option<(i64, String, String)> = None; // (reward, slot_id, model)
    for t in &board {
        if t.publisher == self_id {
            continue;
        }
        // 仅匹配已批准加入该队长团队的任务
        let approved = relay::is_approved_member(&url, &t.publisher, &self_id).await.unwrap_or(false);
        if !approved {
            continue;
        }
        for slot in &t.slots {
            if slot.claimed_by.is_some() {
                continue;
            }
            // 找一个匹配且空闲的本机模型
            let mut pick: Option<String> = None;
            for m in &models {
                let matched =
                    slot.model_filter == "any" || slot.model_filter == m.model || slot.model_filter == m.label;
                if matched && !storage::is_model_busy(&conn, &self_id, &m.model)? {
                    pick = Some(m.model.clone());
                    break;
                }
            }
            if pick.is_some() && best.as_ref().map_or(true, |(r, _, _)| t.reward > *r) {
                best = Some((t.reward, slot.slot_id.clone(), pick.unwrap()));
            }
        }
    }

    if let Some((_, slot_id, model)) = best {
        if relay::claim_slot(&url, &slot_id, &self_id).await? {
            let task_id = slot_id.split('#').next().unwrap_or("").to_string();
            storage::set_model_busy(&conn, &self_id, &model, &task_id)?;
            return Ok(Some(slot_id));
        }
    }
    Ok(None)
}

/// POST /api/match/auto —— 一键自动匹配（手动触发，需求 7）。
async fn match_auto() -> ApiResult {
    match auto_match_once().await {
        Ok(Some(slot)) => Ok(Json(json!({ "ok": true, "claimed": slot }))),
        Ok(None) => Ok(Json(json!({ "ok": true, "claimed": null, "message": "暂无可匹配任务" }))),
        Err(e) => Err(err500(e)),
    }
}

async fn match_hosted_get() -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    let h = storage::is_hosted(&conn).map_err(err500)?;
    Ok(Json(json!({ "hosted": h })))
}

#[derive(Deserialize)]
struct HostedReq {
    enabled: bool,
}

/// PUT /api/match/hosted —— 托管匹配开关（需求 7：开启后空闲自动领取）。
async fn match_hosted(Json(req): Json<HostedReq>) -> ApiResult {
    let conn = storage::open_conn().map_err(err500)?;
    storage::set_hosted(&conn, req.enabled).map_err(err500)?;
    Ok(Json(json!({ "ok": true, "hosted": req.enabled })))
}

/* ---------- 控制台访问控制 ---------- */

/// GET /api/auth/status —— 是否已设置密码 + 密码文件 mtime（公开）。
async fn auth_status() -> Json<Value> {
    Json(json!({
        "passwordSet": auth::is_password_set(),
    }))
}

#[derive(Deserialize)]
struct LoginReq {
    /// 明文密码。
    password: String,
}

/// POST /api/auth/login —— 校验密码，签发 session token（24h 有效）。
///
/// 启动期已保证密码存在；密码错误时返回 401。
async fn auth_login(Json(req): Json<LoginReq>) -> (StatusCode, Json<Value>) {
    if !auth::verify_password(&req.password) {
        // 防爆破：人为加 200ms 抖动（避开 argon2 默认时长的统计差）。
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_password", "message": "密码错误" })),
        );
    }
    let conn = match storage::open_conn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "登录时打开 DB 失败");
            return err500(e);
        }
    };
    match auth::create_session(&conn) {
        Ok((token, expires_at)) => (
            StatusCode::OK,
            Json(json!({
                "token": token,
                "expiresAt": expires_at,
                "ttlSeconds": auth::SESSION_TTL_SECS,
            })),
        ),
        Err(e) => {
            tracing::error!(error = %e, "创建 session 失败");
            err500(e)
        }
    }
}

/// POST /api/auth/logout —— 注销当前 token（受中间件保护，必须带有效 token）。
async fn auth_logout(headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.trim());
    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "missing_token" })),
        );
    };
    let conn = match storage::open_conn() {
        Ok(c) => c,
        Err(e) => return err500(e),
    };
    match auth::delete_session(&conn, token) {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => err500(e),
    }
}
