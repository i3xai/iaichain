//! 本地 HTTP API（axum）。
//!
//! 阶段 0：`/api/health`、`/api/version`，并以 `static_handler` 回退服务内嵌前端。
//! 后续阶段在此挂载 `/api/node`、`/api/wallet`、`/api/ledger`、`/api/market/*`、
//! `/api/team`、`/api/tasks/*` 等端点（见 `DEVELOPMENT-PLAN.md` 各阶段「API」小节）。

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::{embed::static_handler, storage};

/// 启动本地服务，仅绑定回环地址。
pub async fn serve(port: u16) -> anyhow::Result<()> {
    // 阶段 0：初始化存储（即便端点暂未用到，也确保数据目录/库就绪）。
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
        .fallback(static_handler)
}

/// 健康检查。
async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "node": "iai-chain" }))
}

/// 版本信息（前端安装区/落地页据此展示真实版本）。
async fn version() -> Json<Value> {
    Json(json!({
        "name": "iai-chain",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
