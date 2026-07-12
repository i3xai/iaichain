//! 协调中继（阶段 10b / Phase 1）：任务公告板 + 原子领取 + 入队申请板。
//!
//! 节点通过环境变量 `IAI_RELAY=http://<host>:<port>` 连接中继。中继只做发现/协商：
//! 发布公告、拉取公告板、原子占位领取、入队申请与批准状态。**记账仍各节点本地自治**。
//! 中继状态在内存（重启即清空，适合 demo / 单机多实例演示）。

use std::sync::{Arc, Mutex};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 一个招募槽的公告。`slot_id = task_id#role#slot_index`。
#[derive(Clone, Serialize, Deserialize)]
pub struct SlotAd {
    pub slot_id: String,
    pub role: String,
    #[serde(default = "any_filter")]
    pub model_filter: String,
    #[serde(default)]
    pub claimed_by: Option<String>,
}

fn any_filter() -> String {
    "any".to_string()
}

/// 一个网络任务的公告（含开放槽位）。
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskAd {
    pub task_id: String,
    pub title: String,
    pub repo: String,
    #[serde(default)]
    pub reward: i64,
    pub publisher: String,
    pub slots: Vec<SlotAd>,
}

/// 入队申请公告（跨节点权威状态）。
#[derive(Clone, Serialize, Deserialize)]
pub struct JoinAd {
    pub captain: String,
    pub applicant: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub model: String,
    /// pending | approved | rejected
    pub status: String,
}

fn default_role() -> String {
    "开发".to_string()
}

#[derive(Default)]
struct Board {
    tasks: Vec<TaskAd>,
    joins: Vec<JoinAd>,
}

type Shared = Arc<Mutex<Board>>;

/* ---------- 中继服务（iai relay） ---------- */

pub async fn serve_relay(port: u16) -> anyhow::Result<()> {
    let state: Shared = Arc::new(Mutex::new(Board::default()));
    let app = Router::new()
        .route("/relay/health", get(health))
        .route("/relay/publish", post(publish))
        .route("/relay/board", get(board))
        .route("/relay/claim", post(claim))
        .route("/relay/join", post(join_apply))
        .route("/relay/joins", get(joins_list))
        .route("/relay/join/decide", post(join_decide))
        .with_state(state);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("IAI 协调中继已启动 http://{addr}  （任务公告板 + 入队）");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "role": "relay" }))
}

async fn publish(State(s): State<Shared>, Json(ad): Json<TaskAd>) -> Json<Value> {
    let mut b = s.lock().unwrap();
    b.tasks.retain(|t| t.task_id != ad.task_id); // 同 id 覆盖
    b.tasks.push(ad);
    Json(json!({ "ok": true }))
}

async fn board(State(s): State<Shared>) -> Json<Value> {
    let b = s.lock().unwrap();
    Json(serde_json::to_value(&b.tasks).unwrap_or(Value::Array(vec![])))
}

#[derive(Deserialize)]
struct ClaimReq {
    slot_id: String,
    node: String,
}

/// 原子领取：仅当槽未被占时成功（乐观锁，防重复领取）。
async fn claim(State(s): State<Shared>, Json(req): Json<ClaimReq>) -> (StatusCode, Json<Value>) {
    let mut b = s.lock().unwrap();
    for t in b.tasks.iter_mut() {
        for slot in t.slots.iter_mut() {
            if slot.slot_id == req.slot_id {
                if slot.claimed_by.is_some() {
                    return (StatusCode::CONFLICT, Json(json!({ "ok": false, "error": "已被领取" })));
                }
                slot.claimed_by = Some(req.node.clone());
                return (StatusCode::OK, Json(json!({ "ok": true })));
            }
        }
    }
    (StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": "槽位不存在" })))
}

#[derive(Deserialize)]
struct JoinApplyReq {
    captain: String,
    applicant: String,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default)]
    model: String,
}

async fn join_apply(State(s): State<Shared>, Json(req): Json<JoinApplyReq>) -> Json<Value> {
    let mut b = s.lock().unwrap();
    if let Some(j) = b
        .joins
        .iter_mut()
        .find(|j| j.captain == req.captain && j.applicant == req.applicant)
    {
        if j.status != "approved" {
            j.role = req.role;
            j.model = req.model;
            j.status = "pending".into();
        }
    } else {
        b.joins.push(JoinAd {
            captain: req.captain,
            applicant: req.applicant,
            role: req.role,
            model: req.model,
            status: "pending".into(),
        });
    }
    Json(json!({ "ok": true }))
}

#[derive(Deserialize)]
struct JoinsQuery {
    captain: Option<String>,
}

async fn joins_list(State(s): State<Shared>, Query(q): Query<JoinsQuery>) -> Json<Value> {
    let b = s.lock().unwrap();
    let list: Vec<&JoinAd> = match q.captain.as_deref() {
        Some(c) if !c.is_empty() => b.joins.iter().filter(|j| j.captain == c).collect(),
        _ => b.joins.iter().collect(),
    };
    Json(serde_json::to_value(list).unwrap_or(Value::Array(vec![])))
}

#[derive(Deserialize)]
struct JoinDecideReq {
    captain: String,
    applicant: String,
    approve: bool,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

async fn join_decide(State(s): State<Shared>, Json(req): Json<JoinDecideReq>) -> (StatusCode, Json<Value>) {
    let mut b = s.lock().unwrap();
    let Some(j) = b
        .joins
        .iter_mut()
        .find(|j| j.captain == req.captain && j.applicant == req.applicant)
    else {
        // 直接邀请：无申请也可写入 approved
        if req.approve {
            b.joins.push(JoinAd {
                captain: req.captain,
                applicant: req.applicant,
                role: req.role.unwrap_or_else(default_role),
                model: req.model.unwrap_or_default(),
                status: "approved".into(),
            });
            return (StatusCode::OK, Json(json!({ "ok": true })));
        }
        return (StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": "申请不存在" })));
    };
    j.status = if req.approve { "approved" } else { "rejected" }.into();
    if let Some(r) = req.role {
        j.role = r;
    }
    if let Some(m) = req.model {
        j.model = m;
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}

/* ---------- 节点侧客户端（连接中继） ---------- */

/// 中继地址（`IAI_RELAY` 环境变量），未配置返回 None。
pub fn relay_url() -> Option<String> {
    std::env::var("IAI_RELAY").ok().filter(|s| !s.trim().is_empty())
}

pub async fn publish_task(url: &str, ad: &TaskAd) -> anyhow::Result<()> {
    reqwest::Client::new()
        .post(format!("{url}/relay/publish"))
        .json(ad)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn fetch_board(url: &str) -> anyhow::Result<Vec<TaskAd>> {
    let tasks = reqwest::Client::new()
        .get(format!("{url}/relay/board"))
        .send()
        .await?
        .json::<Vec<TaskAd>>()
        .await?;
    Ok(tasks)
}

/// 领取槽位，返回是否成功（冲突/不存在返回 false）。
pub async fn claim_slot(url: &str, slot_id: &str, node: &str) -> anyhow::Result<bool> {
    let res = reqwest::Client::new()
        .post(format!("{url}/relay/claim"))
        .json(&json!({ "slot_id": slot_id, "node": node }))
        .send()
        .await?;
    Ok(res.status().is_success())
}

pub async fn join_apply_remote(
    url: &str,
    captain: &str,
    applicant: &str,
    role: &str,
    model: &str,
) -> anyhow::Result<()> {
    reqwest::Client::new()
        .post(format!("{url}/relay/join"))
        .json(&json!({
            "captain": captain,
            "applicant": applicant,
            "role": role,
            "model": model,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn fetch_joins(url: &str, captain: Option<&str>) -> anyhow::Result<Vec<JoinAd>> {
    let mut req = reqwest::Client::new().get(format!("{url}/relay/joins"));
    if let Some(c) = captain {
        req = req.query(&[("captain", c)]);
    }
    let list = req.send().await?.json::<Vec<JoinAd>>().await?;
    Ok(list)
}

pub async fn join_decide_remote(
    url: &str,
    captain: &str,
    applicant: &str,
    approve: bool,
    role: Option<&str>,
    model: Option<&str>,
) -> anyhow::Result<()> {
    reqwest::Client::new()
        .post(format!("{url}/relay/join/decide"))
        .json(&json!({
            "captain": captain,
            "applicant": applicant,
            "approve": approve,
            "role": role,
            "model": model,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

/// 申请人是否已被该队长批准（中继权威）。
pub async fn is_approved_member(url: &str, captain: &str, applicant: &str) -> anyhow::Result<bool> {
    let joins = fetch_joins(url, Some(captain)).await?;
    Ok(joins
        .iter()
        .any(|j| j.applicant == applicant && j.status == "approved"))
}

/// 根据 slot_id 在公告板找到任务发布者。
pub async fn publisher_of_slot(url: &str, slot_id: &str) -> anyhow::Result<Option<String>> {
    let board = fetch_board(url).await?;
    for t in board {
        if t.slots.iter().any(|s| s.slot_id == slot_id) {
            return Ok(Some(t.publisher));
        }
    }
    Ok(None)
}
