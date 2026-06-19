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
    /// 钱包余额（由账本推导）。
    Wallet,
    /// 账本：流水 / 校验 / 记账。
    Ledger {
        #[command(subcommand)]
        action: LedgerCmd,
    },
    /// 市场：挂卖簿 / 挂卖 / 买入。
    Market {
        #[command(subcommand)]
        action: MarketCmd,
    },
    /// 打印版本号。
    Version,
}

#[derive(Subcommand, Debug)]
pub enum MarketCmd {
    /// 查看挂卖簿（价格升序，最低价在前）。
    Book,
    /// 挂出卖单：iai market sell --px 0.90 --qty 100。
    Sell {
        /// 单价（元/币）。
        #[arg(long)]
        px: f64,
        /// 数量。
        #[arg(long)]
        qty: i64,
        /// 卖方节点（默认本机）。
        #[arg(long)]
        node: Option<String>,
    },
    /// 按最低价买入：iai market buy --qty 120（成交计入账本）。
    Buy {
        #[arg(long)]
        qty: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum LedgerCmd {
    /// 列出最近账本流水（最新在前）。
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// 重算哈希链，校验完整性（防篡改）。
    Verify,
    /// 追加一条账本记录。
    ///
    /// 运维 / 演示入口；阶段 3 市场买卖、阶段 5 任务锁定、阶段 6 结算分发
    /// 将由程序自动调用同一记账接口。
    Record {
        /// 类型：settle / reward / lock / unlock / buy / sell。
        #[arg(long)]
        kind: String,
        /// 可用余额变动（带符号，如 +180 / -80）。
        #[arg(long, allow_hyphen_values = true)]
        amount: i64,
        /// 锁定池变动（默认 0）。
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        locked: i64,
        /// 备注（流水「说明」列）。
        #[arg(long, default_value = "")]
        note: String,
        /// 关联节点（默认本机节点）。
        #[arg(long)]
        node: Option<String>,
    },
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
