//! clap 子命令定义。
//! 阶段 0：`serve` / `version`。阶段 1：`model add`、`node status`。
//! 后续阶段扩展 `wallet` / `ledger` / `market` / `team` / `task`。

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "iai",
    version,
    about = "IAI Chain · 去中心化 AI 能力与任务市场节点"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 启动本地节点：内嵌前端 + 本地 HTTP API。
    Serve {
        /// 监听端口（仅绑定回环地址 127.0.0.1）。
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// 模型配置：让本机成为具备 AI 能力的节点。
    Model {
        #[command(subcommand)]
        action: ModelCmd,
    },
    /// 本机节点信息。
    Node {
        #[command(subcommand)]
        action: NodeCmd,
    },
    /// 打印版本号。
    Version,
}

#[derive(Subcommand, Debug)]
pub enum ModelCmd {
    /// 新增一个大模型：`iai model add openai --key sk-****`。
    Add {
        /// Provider：openai / anthropic / ollama / 其他。
        provider: String,
        /// 模型名（省略则用 Provider 默认模型）。
        #[arg(long)]
        model: Option<String>,
        /// API key（本地 Provider 如 ollama 可省略）。
        #[arg(long)]
        key: Option<String>,
    },
    /// 列出已配置模型（不显示 key）。
    List,
}

#[derive(Subcommand, Debug)]
pub enum NodeCmd {
    /// 显示本机节点身份与状态。
    Status,
}
