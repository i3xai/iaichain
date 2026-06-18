//! clap 子命令定义。阶段 0 仅 `serve` 与 `version`；后续阶段在此扩展
//! `model` / `wallet` / `ledger` / `market` / `team` / `task` 等子命令。

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
    /// 打印版本号。
    Version,
}
