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
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub reject_reason: String,
}

fn default_role() -> String {
    "开发".to_string()
}

/// 任务角色招募申请（跨节点）。
#[derive(Clone, Serialize, Deserialize)]
pub struct RecruitAd {
    pub task_id: String,
    pub publisher: String,
    pub role: String,
    pub applicant: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub model: String,
    /// pending | approved | rejected
    pub status: String,
    #[serde(default)]
    pub reject_reason: String,
}

#[derive(Default)]
struct Board {
    tasks: Vec<TaskAd>,
    joins: Vec<JoinAd>,
    recruits: Vec<RecruitAd>,
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
        .route("/relay/recruit", post(recruit_apply))
        .route("/relay/recruits", get(recruits_list))
        .route("/relay/recruit/decide", post(recruit_decide))
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
    #[serde(default)]
    message: String,
}

async fn join_apply(State(s): State<Shared>, Json(req): Json<JoinApplyReq>) -> (StatusCode, Json<Value>) {
    let mut b = s.lock().unwrap();
    if let Some(j) = b
        .joins
        .iter_mut()
        .find(|j| j.captain == req.captain && j.applicant == req.applicant)
    {
        if j.status == "approved" {
            return (StatusCode::OK, Json(json!({ "ok": true, "status": "approved" })));
        }
        if j.status == "rejected" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "error": "申请已被拒绝，本次不可再次申请" })),
            );
        }
        j.role = req.role;
        j.model = req.model;
        j.message = req.message;
        j.status = "pending".into();
    } else {
        b.joins.push(JoinAd {
            captain: req.captain,
            applicant: req.applicant,
            role: req.role,
            model: req.model,
            status: "pending".into(),
            message: req.message,
            reject_reason: String::new(),
        });
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
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
    #[serde(default)]
    reject_reason: Option<String>,
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
                message: String::new(),
                reject_reason: String::new(),
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
    if !req.approve {
        j.reject_reason = req.reject_reason.unwrap_or_default();
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RecruitApplyReq {
    task_id: String,
    publisher: String,
    role: String,
    applicant: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    model: String,
}

async fn recruit_apply(State(s): State<Shared>, Json(req): Json<RecruitApplyReq>) -> (StatusCode, Json<Value>) {
    let mut b = s.lock().unwrap();
    // 校验任务仍有未领槽
    let open = b
        .tasks
        .iter()
        .find(|t| t.task_id == req.task_id)
        .map(|t| {
            t.slots
                .iter()
                .any(|s| s.role == req.role && s.claimed_by.is_none())
        })
        .unwrap_or(false);
    if !open {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "该角色已无空余招募名额或任务不存在" })),
        );
    }
    if let Some(r) = b.recruits.iter_mut().find(|r| {
        r.task_id == req.task_id && r.role == req.role && r.applicant == req.applicant
    }) {
        if r.status == "approved" {
            return (StatusCode::OK, Json(json!({ "ok": true, "status": "approved" })));
        }
        if r.status == "rejected" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "error": "该角色申请已被拒绝，本次不可再次申请" })),
            );
        }
        r.message = req.message;
        r.model = req.model;
        r.status = "pending".into();
    } else {
        b.recruits.push(RecruitAd {
            task_id: req.task_id,
            publisher: req.publisher,
            role: req.role,
            applicant: req.applicant,
            message: req.message,
            model: req.model,
            status: "pending".into(),
            reject_reason: String::new(),
        });
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RecruitsQuery {
    publisher: Option<String>,
    task_id: Option<String>,
}

async fn recruits_list(State(s): State<Shared>, Query(q): Query<RecruitsQuery>) -> Json<Value> {
    let b = s.lock().unwrap();
    let list: Vec<&RecruitAd> = b
        .recruits
        .iter()
        .filter(|r| {
            if let Some(p) = q.publisher.as_deref() {
                if !p.is_empty() && r.publisher != p {
                    return false;
                }
            }
            if let Some(t) = q.task_id.as_deref() {
                if !t.is_empty() && r.task_id != t {
                    return false;
                }
            }
            true
        })
        .collect();
    Json(serde_json::to_value(list).unwrap_or(Value::Array(vec![])))
}

#[derive(Deserialize)]
struct RecruitDecideReq {
    task_id: String,
    role: String,
    applicant: String,
    approve: bool,
    #[serde(default)]
    reject_reason: Option<String>,
}

async fn recruit_decide(State(s): State<Shared>, Json(req): Json<RecruitDecideReq>) -> (StatusCode, Json<Value>) {
    let mut b = s.lock().unwrap();
    let exists_pending = b.recruits.iter().any(|r| {
        r.task_id == req.task_id
            && r.role == req.role
            && r.applicant == req.applicant
            && r.status == "pending"
    });
    if !exists_pending {
        let any = b.recruits.iter().any(|r| {
            r.task_id == req.task_id && r.role == req.role && r.applicant == req.applicant
        });
        if !any {
            return (StatusCode::NOT_FOUND, Json(json!({ "ok": false, "error": "招募申请不存在" })));
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "申请已处理" })),
        );
    }
    if req.approve {
        let mut claimed = false;
        for t in b.tasks.iter_mut() {
            if t.task_id != req.task_id {
                continue;
            }
            if let Some(slot) = t
                .slots
                .iter_mut()
                .find(|s| s.role == req.role && s.claimed_by.is_none())
            {
                slot.claimed_by = Some(req.applicant.clone());
                claimed = true;
                break;
            }
        }
        if !claimed {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "ok": false, "error": "该角色已无空余槽位" })),
            );
        }
    }
    if let Some(r) = b.recruits.iter_mut().find(|r| {
        r.task_id == req.task_id && r.role == req.role && r.applicant == req.applicant
    }) {
        if req.approve {
            r.status = "approved".into();
        } else {
            r.status = "rejected".into();
            r.reject_reason = req.reject_reason.unwrap_or_default();
        }
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
    message: &str,
) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .post(format!("{url}/relay/join"))
        .json(&json!({
            "captain": captain,
            "applicant": applicant,
            "role": role,
            "model": model,
            "message": message,
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        let v: Value = res.json().await.unwrap_or(json!({}));
        anyhow::bail!(v["error"].as_str().unwrap_or("入队申请失败").to_string());
    }
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
    reject_reason: Option<&str>,
) -> anyhow::Result<()> {
    reqwest::Client::new()
        .post(format!("{url}/relay/join/decide"))
        .json(&json!({
            "captain": captain,
            "applicant": applicant,
            "approve": approve,
            "role": role,
            "model": model,
            "reject_reason": reject_reason,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn recruit_apply_remote(
    url: &str,
    task_id: &str,
    publisher: &str,
    role: &str,
    applicant: &str,
    message: &str,
    model: &str,
) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .post(format!("{url}/relay/recruit"))
        .json(&json!({
            "task_id": task_id,
            "publisher": publisher,
            "role": role,
            "applicant": applicant,
            "message": message,
            "model": model,
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        let v: Value = res.json().await.unwrap_or(json!({}));
        anyhow::bail!(v["error"].as_str().unwrap_or("招募申请失败").to_string());
    }
    Ok(())
}

pub async fn fetch_recruits(
    url: &str,
    publisher: Option<&str>,
    task_id: Option<&str>,
) -> anyhow::Result<Vec<RecruitAd>> {
    let mut req = reqwest::Client::new().get(format!("{url}/relay/recruits"));
    let mut q: Vec<(&str, &str)> = Vec::new();
    if let Some(p) = publisher {
        q.push(("publisher", p));
    }
    if let Some(t) = task_id {
        q.push(("task_id", t));
    }
    if !q.is_empty() {
        req = req.query(&q);
    }
    Ok(req.send().await?.json::<Vec<RecruitAd>>().await?)
}

pub async fn recruit_decide_remote(
    url: &str,
    task_id: &str,
    role: &str,
    applicant: &str,
    approve: bool,
    reject_reason: Option<&str>,
) -> anyhow::Result<()> {
    let res = reqwest::Client::new()
        .post(format!("{url}/relay/recruit/decide"))
        .json(&json!({
            "task_id": task_id,
            "role": role,
            "applicant": applicant,
            "approve": approve,
            "reject_reason": reject_reason,
        }))
        .send()
        .await?;
    if !res.status().is_success() {
        let v: Value = res.json().await.unwrap_or(json!({}));
        anyhow::bail!(v["error"].as_str().unwrap_or("审批失败").to_string());
    }
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
