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
    /// 团队：创建招募 / 邀请成员 / 成员列表。
    Team {
        #[command(subcommand)]
        action: TeamCmd,
    },
    /// 网络概况（在线成员 / 已知节点 / 公开团队）。
    Net,
    /// 任务：发起 / 列表 / 详情。
    Task {
        #[command(subcommand)]
        action: TaskCmd,
    },
    /// 控制台访问密码：set / status / clear。
    Password {
        #[command(subcommand)]
        action: PasswordCmd,
    },
    /// 在线升级：从 GitHub Releases 拉取最新版本并替换当前二进制。
    Upgrade {
        #[command(subcommand)]
        action: UpgradeCmd,
    },
    /// 打印版本号。
    Version,
}

#[derive(Subcommand, Debug)]
pub enum UpgradeCmd {
    /// 仅检查是否有新版本（不下载、不安装）。
    Check,
    /// 检查并升级到最新版本（默认行为）。可用 --to 指定目标版本。
    Run {
        /// 指定目标版本 tag（如 `v0.5.0`）；省略则用 latest release。
        #[arg(long)]
        to: Option<String>,
        /// 跳过确认提示直接升级。
        #[arg(long, short = 'y')]
        yes: bool,
        /// 安装后不自动重启 systemd 服务（默认会自动 restart iai.service）。
        #[arg(long)]
        no_restart: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    /// 发起任务：iai task run --repo github.com/acme/x --prompt "实现限流中间件"。
    Run {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        prompt: String,
    },
    /// 任务列表。
    List,
    /// 任务详情（含聚合结果）。
    Status {
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TeamCmd {
    /// 创建团队并发布招募：iai team create --recruit "需要 Rust 限流中间件"。
    Create {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        recruit: String,
    },
    /// 邀请 / 登记成员节点：iai team invite --node node.4a91 --role 后端 --model "Claude 3.5"。
    Invite {
        #[arg(long)]
        node: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        model: String,
        /// 成员累计贡献点（自报）。
        #[arg(long, default_value_t = 0)]
        credits: i64,
        /// 标记为离线。
        #[arg(long)]
        offline: bool,
    },
    /// 列出团队成员。
    List,
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

#[derive(Subcommand, Debug)]
pub enum PasswordCmd {
    /// 设置或更新控制台访问密码。
    ///
    /// 默认交互式（两次输入隐藏密码）；用 `--stdin` 从标准输入读单行密码，
    /// 适合脚本/自动化场景。密码长度至少 8 位。
    ///
    /// 设新密码后会清空一次性明文文件（CONSOLE_PASSWORD.txt）。
    Set {
        /// 从 stdin 读取密码（单行；末尾换行会被截断）。
        #[arg(long)]
        stdin: bool,
    },
    /// 显示初始随机密码（仅当一次性文件 CONSOLE_PASSWORD.txt 还存在时可用）。
    ///
    /// 文件被 `iai password set` / `iai password reset` 删除后，本命令会提示需要重置密码。
    Show,
    /// 重置密码为新的随机密码（生成新的强密码并清空所有 session）。
    ///
    /// 新密码会重新写入一次性明文文件供管理员取走；旧 session 立即失效。
    Reset,
    /// 查看当前密码状态（是否设置 + 活跃 session 数 + 一次性文件是否在）。
    Status,
}
