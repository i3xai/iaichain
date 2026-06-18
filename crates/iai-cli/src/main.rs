//! IAI Chain 节点二进制 `iai`。
//!
//! 阶段 0：`iai serve` —— 本地 HTTP 服务 + 内嵌前端。
//! 阶段 1：`iai model add` / `iai node status` —— 配置模型、查看本机节点身份。

mod cli;
mod embed;
mod storage;
mod api;

use clap::Parser;
use cli::{Cli, Command, ModelCmd, NodeCmd};
use iai_node::Provider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve { port } => api::serve(port).await,
        Command::Model { action } => run_model(action),
        Command::Node { action } => run_node(action),
        Command::Version => {
            println!("iai-chain {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn run_model(action: ModelCmd) -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    storage::ensure_node(&conn)?;
    match action {
        ModelCmd::Add { provider, model, key } => {
            let provider = Provider::parse(&provider);
            let model = model
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| provider.default_model().to_string());
            if provider.requires_key() && key.as_deref().map(str::trim).unwrap_or("").is_empty() {
                anyhow::bail!("{} 需要 --key", provider.display());
            }
            let saved = storage::add_model(&conn, &provider, &model, key.as_deref())?;
            println!("✓ 已配置模型: {}", saved.label);
        }
        ModelCmd::List => {
            let models = storage::list_models(&conn)?;
            if models.is_empty() {
                println!("（暂无已配置模型，用 `iai model add <provider> --key …` 添加）");
            } else {
                for m in models {
                    println!("· {}", m.label);
                }
            }
        }
    }
    Ok(())
}

fn run_node(action: NodeCmd) -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    storage::ensure_node(&conn)?;
    match action {
        NodeCmd::Status => {
            let n = storage::get_node(&conn)?.expect("节点已确保存在");
            let models = storage::list_models(&conn)?;
            let caps = storage::capabilities(&conn)?;
            println!("节点      {}", n.node_id);
            println!("角色      {}", n.role.display_zh());
            println!("状态      {}", if n.status.is_online() { "在线" } else { "离线" });
            println!(
                "模型      {}",
                if models.is_empty() {
                    "未配置".to_string()
                } else {
                    models.iter().map(|m| m.label.clone()).collect::<Vec<_>>().join(" · ")
                }
            );
            println!(
                "能力      {}",
                if caps.is_empty() { "—（需先配置模型）".to_string() } else { caps.join(", ") }
            );
        }
    }
    Ok(())
}
