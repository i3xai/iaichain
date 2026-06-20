//! 协调中继（阶段 10b）：去中心化网络的**任务公告板 + 原子领取**。
//!
//! 节点通过环境变量 `IAI_RELAY=http://<host>:<port>` 连接中继。中继只做发现/协商：
//! 发布公告、拉取公告板、原子占位领取（防重复）。**记账仍各节点本地自治**，不经中继。
//! 中继状态在内存（重启即清空，适合 demo / 单机多实例演示）。

use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
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

#[derive(Default)]
struct Board {
    tasks: Vec<TaskAd>,
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
        .with_state(state);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("IAI 协调中继已启动 http://{addr}  （任务公告板）");
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
