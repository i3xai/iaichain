//! IAI Chain 节点二进制 `iai`。
//!
//! 阶段 0：`iai serve` —— 本地 HTTP 服务 + 内嵌前端。
//! 阶段 1：`iai model add` / `iai node status` —— 配置模型、查看本机节点身份。

mod cli;
mod embed;
mod orchestrator;
mod relay;
mod storage;
mod api;
mod auth;
mod upgrade;

use clap::Parser;
use cli::{Cli, Command, LedgerCmd, MarketCmd, ModelCmd, NodeCmd, PasswordCmd, TaskCmd, TeamCmd, UpgradeCmd};
use iai_economic::{credit, ledger, ledger::LedgerKind, market};
use iai_node::{registry, Provider};
use rusqlite::params;

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
        Command::Relay { port } => relay::serve_relay(port).await,
        Command::Model { action } => run_model(action),
        Command::Node { action } => run_node(action),
        Command::Wallet => run_wallet(),
        Command::Ledger { action } => run_ledger(action),
        Command::Market { action } => run_market(action),
        Command::Team { action } => run_team(action),
        Command::Net => run_net(),
        Command::Task { action } => run_task_cmd(action).await,
        Command::Password { action } => run_password(action),
        Command::Upgrade { action } => run_upgrade(action).await,
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

fn run_wallet() -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    let self_id = storage::ensure_node(&conn)?;
    let entries = storage::entries_for(&conn, &self_id)?;
    let w = credit::derive_wallet(&entries, storage::now_epoch());
    println!("可用余额  {}", w.balance);
    println!("任务锁定  {}", w.locked);
    println!("本周收益  +{}（{} 笔被采纳）", w.weekly, w.weekly_accepted);
    Ok(())
}

fn signed(n: i64) -> String {
    if n >= 0 { format!("+{n}") } else { n.to_string() }
}

fn run_ledger(action: LedgerCmd) -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    match action {
        LedgerCmd::List { limit } => {
            let self_id = storage::ensure_node(&conn)?;
            let entries = storage::list_ledger_desc_for(&conn, &self_id, limit)?;
            if entries.is_empty() {
                println!("（账本为空，用 `iai ledger record …` 记账）");
            } else {
                for e in entries {
                    println!(
                        "{}  {:<4} {:>7}  {}",
                        ledger::display_time(e.ts_epoch),
                        e.kind.display_zh(),
                        signed(e.amount),
                        e.note
                    );
                }
            }
        }
        LedgerCmd::Verify => {
            let entries = storage::all_entries_asc(&conn)?;
            match ledger::verify_chain(&entries) {
                Ok(()) => println!("✓ 账本链完整 · {} 条记录", entries.len()),
                Err(e) => {
                    eprintln!("✗ 校验失败: {e}");
                    std::process::exit(1);
                }
            }
        }
        LedgerCmd::Record { kind, amount, locked, note, node } => {
            let kind = LedgerKind::from_str(&kind)
                .ok_or_else(|| anyhow::anyhow!("未知账本类型: {kind}（settle/reward/lock/unlock/buy/sell）"))?;
            let node_id = match node {
                Some(n) => n,
                None => storage::ensure_node(&conn)?,
            };
            let e = storage::append_entry(&conn, kind, &node_id, amount, locked, &note)?;
            println!("✓ 已记账 seq={} {} {}", e.seq, e.kind.display_zh(), signed(e.amount));
        }
    }
    Ok(())
}

fn run_market(action: MarketCmd) -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    match action {
        MarketCmd::Book => {
            let asks = storage::list_asks_asc(&conn)?;
            if asks.is_empty() {
                println!("（挂卖簿为空，用 `iai market sell --px <价> --qty <量>` 挂单）");
            } else {
                for a in asks {
                    println!("¥{:.2}  ×{:<6} {}", market::yuan(a.px_cents), a.qty, a.node_id);
                }
            }
        }
        MarketCmd::Sell { px, qty, node } => {
            let node_id = match node {
                Some(n) => n,
                None => storage::ensure_node(&conn)?,
            };
            let a = storage::add_ask(&conn, market::cents_from_yuan(px), qty, &node_id)?;
            println!("✓ 已挂卖 {} 币 @ ¥{:.2} · {}", a.qty, market::yuan(a.px_cents), a.node_id);
        }
        MarketCmd::Buy { qty } => {
            let node = storage::ensure_node(&conn)?;
            let out = storage::execute_buy(&conn, &node, qty)?;
            if out.filled == 0 {
                println!("无可成交挂单");
            } else {
                println!(
                    "✓ 成交 {} 币 · ¥{:.2}（已计入账本买入）",
                    out.filled,
                    market::yuan(out.cost_cents)
                );
            }
        }
    }
    Ok(())
}

fn run_team(action: TeamCmd) -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    match action {
        TeamCmd::Create { name, recruit } => {
            let nm = name.unwrap_or_else(|| "我的团队".to_string());
            let id = storage::create_team(&conn, &nm, &recruit)?;
            println!("✓ 团队 #{id}「{nm}」已创建 · 招募：{recruit}");
        }
        TeamCmd::Invite { node, role, model, credits, offline } => {
            storage::invite_member(&conn, &node, &role, &model, credits, !offline)?;
            println!("✓ 已邀请成员 {node} · {role} · {model}");
        }
        TeamCmd::List => {
            for m in storage::list_team(&conn)? {
                let name = if m.is_self { format!("本机 · {}", m.node_id) } else { m.node_id.clone() };
                let credits = if m.is_self { "—".to_string() } else { registry::format_credits(m.credits) };
                println!(
                    "{:<24} {:<6} {:<22} {} {}",
                    name,
                    m.role,
                    m.model,
                    if m.online { "在线" } else { "离线" },
                    credits
                );
            }
        }
    }
    Ok(())
}

fn run_net() -> anyhow::Result<()> {
    let conn = storage::open_conn()?;
    let s = storage::network_stat(&conn)?;
    println!("在线成员  {}", s.members_online);
    println!("已知节点  {}", s.discovered);
    println!("公开团队  {}", s.public_teams);
    Ok(())
}

fn run_password(action: PasswordCmd) -> anyhow::Result<()> {
    match action {
        PasswordCmd::Set { stdin } => {
            let plain = if stdin {
                use std::io::Read;
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s.trim_end_matches(['\n', '\r']).to_string()
            } else {
                let p1 = rpassword::prompt_password("新密码（≥8 位，输入隐藏）: ")?;
                if p1.len() < 8 {
                    anyhow::bail!("密码至少 8 位");
                }
                let p2 = rpassword::prompt_password("再输一次确认: ")?;
                if p1 != p2 {
                    anyhow::bail!("两次输入不一致");
                }
                p1
            };
            if plain.is_empty() {
                anyhow::bail!("密码不能为空");
            }
            auth::set_password(&plain)?;
            // 改密码同时清掉所有 session，强制重新登录。
            let conn = storage::open_conn()?;
            conn.execute("DELETE FROM auth_sessions", [])?;
            println!("✓ 控制台密码已更新。旧 session 全部失效，需要重新登录。");
            println!("  一次性明文文件已删除；下次忘记密码可用 `iai password reset` 重置。");
        }
        PasswordCmd::Show => {
            match auth::read_initial_password() {
                Some(plain) => {
                    println!("⚠ 一次性明文文件内容（看完请执行 `iai password reset` 删除）:");
                    println!("    {plain}");
                    println!("  文件路径: {}", auth::initial_password_file().display());
                }
                None => {
                    println!("明文密码已不可恢复（一次性文件已被删除或从未生成）。");
                    println!("提示: 运行 `iai password reset` 生成新的随机密码。");
                }
            }
        }
        PasswordCmd::Reset => {
            let new_plain = auth::reset_to_random()?;
            println!("⚠ 控制台密码已重置为新的随机密码:");
            println!("    {new_plain}");
            println!("  已写入一次性明文文件: {}", auth::initial_password_file().display());
            println!("  请妥善保存并删除该文件（看完后运行 `rm $(iai password status | grep 明文 | awk '{{print $NF}}')` 或手动 rm）。");
        }
        PasswordCmd::Status => {
            let path = auth::password_file();
            println!("密码哈希    {}", path.display());
            println!("已设置      {}", if auth::is_password_set() { "是" } else { "否" });
            if let Ok(meta) = std::fs::metadata(&path) {
                if let Ok(mtime) = meta.modified() {
                    if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                        let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(dur.as_secs() as i64, 0)
                            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_default();
                        println!("最近修改    {dt}");
                    }
                }
            }
            println!("明文文件    {}", auth::initial_password_file().display());
            println!("  存在      {}", if auth::has_initial_password_file() { "是（可 `iai password show` 查看）" } else { "否" });

            let conn = storage::open_conn()?;
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM auth_sessions WHERE expires_at > ?",
                params![storage::now_epoch()],
                |r| r.get(0),
            )?;
            println!("活跃会话    {n}");
        }
    }
    Ok(())
}

async fn run_task_cmd(action: TaskCmd) -> anyhow::Result<()> {
    match action {
        TaskCmd::Run { repo, prompt } => {
            let task_id = {
                let conn = storage::open_conn()?;
                orchestrator::create_task(&conn, &prompt, &repo)?
            };
            println!("✓ 任务 {task_id} 已创建并分派，执行中…");
            orchestrator::drive(task_id.clone()).await?;
            let conn = storage::open_conn()?;
            if let Some(t) = storage::get_task(&conn, &task_id)? {
                println!("状态 {}", t.state.display_zh());
                for s in storage::list_subtasks(&conn, &task_id)? {
                    println!(
                        "  {:<6} [{}] {}",
                        s.role,
                        s.status,
                        s.assigned_node.unwrap_or_else(|| "-".into())
                    );
                }
            }
        }
        TaskCmd::List => {
            let conn = storage::open_conn()?;
            let tasks = storage::list_tasks(&conn)?;
            if tasks.is_empty() {
                println!("（暂无任务，用 `iai task run --repo … --prompt …` 发起）");
            }
            for t in tasks {
                let subs = storage::list_subtasks(&conn, &t.task_id)?;
                let done = subs.iter().filter(|s| s.status == "done").count();
                println!(
                    "{}  {:<6} {}/{} 子任务  {}",
                    t.task_id,
                    t.state.display_zh(),
                    done,
                    subs.len(),
                    t.title
                );
            }
        }
        TaskCmd::Status { id } => {
            let conn = storage::open_conn()?;
            let t = match storage::get_task(&conn, &id)? {
                Some(t) => t,
                None => {
                    println!("任务不存在: {id}");
                    return Ok(());
                }
            };
            println!("任务 {}  状态 {}", t.task_id, t.state.display_zh());
            println!("仓库 {}", t.repo);
            for s in storage::list_subtasks(&conn, &id)? {
                let retry = if s.attempts > 1 { format!(" (重试 {})", s.attempts) } else { String::new() };
                println!(
                    "  {:<6} [{}] {}{}",
                    s.role,
                    s.status,
                    s.assigned_node.unwrap_or_else(|| "-".into()),
                    retry
                );
            }
            if let Some(r) = t.result {
                if !r.is_empty() {
                    println!("--- 聚合结果 ---\n{r}");
                }
            }
        }
    }
    Ok(())
}

async fn run_upgrade(action: UpgradeCmd) -> anyhow::Result<()> {
    match action {
        UpgradeCmd::Check => {
            let info = upgrade::check().await?;
            println!("当前版本  v{}", info.current);
            println!("最新版本  {} (v{})", info.latest_tag, info.latest);
            if let Some(p) = info.published_at {
                println!("发布时间  {p}");
            }
            println!("目标平台  {}", info.target);
            println!("资产      {}", info.asset_name);
            println!("SHA256    {}", if info.has_sha256 { "有" } else { "无（仅 TLS 校验）" });
            println!();
            if info.has_update {
                println!("↑ 有新版本可用 → 运行 `iai upgrade` 或 `iai upgrade --to {}`", info.latest_tag);
            } else {
                println!("✓ 已是最新版本");
            }
        }
        UpgradeCmd::Run { to, yes, no_restart } => {
            upgrade::run(to, yes, no_restart).await?;
        }
    }
    Ok(())
}
