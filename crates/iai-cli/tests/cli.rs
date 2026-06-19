//! 黑盒 CLI 集成测试：以真实 `iai` 二进制 + 隔离的临时 IAI_HOME 跑端到端链路，
//! 覆盖 节点/模型、哈希链账本、市场撮合、任务编排与结算闭环（章程 II/IV/V/VI）。

use std::path::PathBuf;
use std::process::{Command, Output};

/// 每个测试一个唯一的临时数据目录，互不干扰。
fn temp_home() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut p = std::env::temp_dir();
    p.push(format!("iai-it-{}-{}", std::process::id(), nanos));
    p
}

fn iai(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_iai"))
        .env("IAI_HOME", home)
        .env("RUST_LOG", "error")
        .args(args)
        .output()
        .expect("运行 iai 失败")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

#[test]
fn node_status_reflects_configured_model() {
    let home = temp_home();
    let o = iai(&home, &["model", "add", "openai", "--key", "sk-test"]);
    assert!(o.status.success());
    assert!(out(&o).contains("OpenAI"));

    let s = out(&iai(&home, &["node", "status"]));
    assert!(s.contains("队长"), "应为队长角色: {s}");
    assert!(s.contains("gpt-4o"), "应显示默认模型: {s}");
    assert!(s.contains("reasoning"), "配置模型后应具备能力: {s}");
}

#[test]
fn model_add_requires_key_for_remote_provider() {
    let home = temp_home();
    let o = iai(&home, &["model", "add", "openai"]);
    assert!(!o.status.success(), "缺 key 应失败");
    assert!(String::from_utf8_lossy(&o.stderr).contains("需要 --key"));
    // 本地 Provider 无需 key
    assert!(iai(&home, &["model", "add", "ollama", "--model", "qwen"]).status.success());
}

#[test]
fn ledger_records_derive_wallet_and_chain_verifies() {
    let home = temp_home();
    iai(&home, &["ledger", "record", "--kind", "settle", "--amount", "180", "--note", "采纳"]);
    iai(&home, &["ledger", "record", "--kind", "buy", "--amount", "120", "--note", "买入"]);

    let w = out(&iai(&home, &["wallet"]));
    assert!(w.contains("可用余额  300"), "余额应为 180+120: {w}");

    let v = out(&iai(&home, &["ledger", "verify"]));
    assert!(v.contains("账本链完整"), "链应完整: {v}");
}

#[test]
fn market_buy_consumes_from_lowest_and_records_purchase() {
    let home = temp_home();
    iai(&home, &["market", "sell", "--px", "0.90", "--qty", "100", "--node", "node.x"]);
    iai(&home, &["market", "sell", "--px", "0.86", "--qty", "50", "--node", "node.y"]);

    // 买 60：从最低 0.86×50 + 0.90×10 = ¥52.00
    let buy = out(&iai(&home, &["market", "buy", "--qty", "60"]));
    assert!(buy.contains("成交 60"), "应成交 60: {buy}");
    assert!(buy.contains("52.00"), "成交额应为 ¥52.00: {buy}");

    // 本机买入 +60 计入钱包
    assert!(out(&iai(&home, &["wallet"])).contains("可用余额  60"));
    assert!(out(&iai(&home, &["ledger", "verify"])).contains("账本链完整"));
}

#[test]
fn task_run_rejects_non_github_repo() {
    let home = temp_home();
    let o = iai(&home, &["task", "run", "--repo", "gitlab.com/x/y", "--prompt", "做点东西"]);
    assert!(!o.status.success(), "非 GitHub 仓库应被拒");
    assert!(String::from_utf8_lossy(&o.stderr).contains("公开 GitHub"));
}

#[test]
fn task_run_completes_lifecycle_and_settles() {
    let home = temp_home();
    iai(&home, &["model", "add", "openai", "--key", "sk"]);
    iai(&home, &["team", "invite", "--node", "node.4a91", "--role", "后端", "--model", "Claude", "--credits", "1000"]);
    iai(&home, &["team", "invite", "--node", "node.b3df", "--role", "测试", "--model", "DeepSeek", "--credits", "500"]);

    let run = out(&iai(&home, &["task", "run", "--repo", "github.com/acme/auth-lib", "--prompt", "实现 JWT 鉴权模块"]));
    assert!(run.contains("已结算"), "任务应推进到已结算: {run}");
    assert!(run.matches("[done]").count() >= 2, "各子任务应完成: {run}");

    // 结算后全链仍可校验（含成员 settle 条目）
    assert!(out(&iai(&home, &["ledger", "verify"])).contains("账本链完整"));

    // 成员累计贡献应较初始增长
    let team = out(&iai(&home, &["team", "list"]));
    assert!(team.contains("node.4a91"));
    assert!(!team.contains("1,000"), "node.4a91 贡献应从 1,000 增长: {team}");
}
