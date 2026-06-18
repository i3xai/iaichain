//! IAI Chain 节点二进制 `iai`。
//!
//! 阶段 0：提供 `iai serve` —— 启动本地 HTTP 服务，内嵌前端静态资源（`web/`）并暴露
//! `/api/health`、`/api/version`。这是「前端 ↔ 本地 API ↔ 引擎」接缝的服务端落点。

mod cli;
mod embed;
mod storage;
mod api;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 结构化日志：默认 info，可用 RUST_LOG 覆盖（章程 VI 可观测）。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve { port } => api::serve(port).await,
        Command::Version => {
            println!("iai-chain {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
