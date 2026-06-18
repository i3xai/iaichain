//! 前端静态资源内嵌与服务。
//!
//! 用 `rust-embed` 把 `web/` 整个目录编译进二进制，实现「单一静态二进制」目标
//! （`DEVELOPMENT-PLAN.md` 阶段 7）。落地页位于 `web/landing/`，控制台位于 `web/console/`，
//! 两者共享 `web/shared/`。

use axum::{
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// 内嵌 `web/` 目录（路径相对于本 crate 的 Cargo.toml）。
#[derive(RustEmbed)]
#[folder = "../../web/"]
struct WebAssets;

/// 静态资源回退处理器：未命中 `/api/*` 路由的请求都到这里。
///
/// - `/`            → 落地页 `landing/index.html`
/// - `/console`     → 控制台 `console/console.html`
/// - 其余路径       → 按原样在 `web/` 下查找
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let resolved = match path {
        "" => "landing/index.html",
        "console" | "console/" => "console/console.html",
        other => other,
    };
    serve(resolved)
}

fn serve(path: &str) -> Response {
    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 · 资源不存在").into_response(),
    }
}
