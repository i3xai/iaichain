//! 任务编排驱动（阶段 5）。
//!
//! 同步部分 [`create_task`]：解析 → 分解 → 匹配（到 Matched）。
//! 异步部分 [`drive`]：逐子任务执行（确定性 MockProvider，模拟耗时，质量门禁 + 失败重试）
//! → 聚合 → Aggregated（已采纳）。结算记账分发与 SSE 实时推送在阶段 6 落地。

use std::time::Duration;

use anyhow::Context;
use iai_core::{
    aggregate,
    decompose::decompose,
    lifecycle::TaskState,
    matcher::match_node,
    provider::{ExecRequest, ModelProvider, MockProvider},
    quality,
};
use rusqlite::Connection;

use crate::storage;

/// 公开仓库守卫：仅允许公开 GitHub 仓库（或同源远程服务器）。
pub fn validate_repo(repo: &str) -> anyhow::Result<()> {
    let r = repo.trim();
    if r.is_empty() {
        anyhow::bail!("缺少 --repo");
    }
    if !r.contains("github.com") {
        anyhow::bail!("仅支持公开 GitHub 仓库（地址需含 github.com）");
    }
    Ok(())
}

fn make_title(prompt: &str) -> String {
    let t = prompt.trim();
    let max = 30;
    if t.chars().count() > max {
        let s: String = t.chars().take(max).collect();
        format!("{s}…")
    } else {
        t.to_string()
    }
}

/// 创建任务并完成 解析 → 分解 → 匹配（同步，结束于 Matched 状态）。返回 task_id。
pub fn create_task(conn: &Connection, prompt: &str, repo: &str) -> anyhow::Result<String> {
    validate_repo(repo)?;
    let title = make_title(prompt);
    let task_id = storage::create_task(conn, &title, repo, prompt, TaskState::Created)?;

    storage::set_task_state(conn, &task_id, TaskState::Parsed)?;
    let specs = decompose(prompt);
    storage::set_task_state(conn, &task_id, TaskState::Decomposed)?;

    let members = storage::list_team(conn)?;
    for (i, spec) in specs.iter().enumerate() {
        let node = match_node(&spec.role, &members);
        storage::add_subtask(conn, &task_id, i as i64, &spec.role, node.as_deref())?;
    }
    storage::set_task_state(conn, &task_id, TaskState::Matched)?;
    Ok(task_id)
}

/// 异步驱动：Matched → 逐子任务执行（失败重试一次）→ 聚合 → Aggregated。
pub async fn drive(task_id: String) -> anyhow::Result<()> {
    let provider = MockProvider;
    let task = {
        let c = storage::open_conn()?;
        storage::get_task(&c, &task_id)?.context("任务不存在")?
    };
    {
        let c = storage::open_conn()?;
        storage::set_task_state(&c, &task_id, TaskState::Executed)?;
    }
    let subs = {
        let c = storage::open_conn()?;
        storage::list_subtasks(&c, &task_id)?
    };

    let mut parts: Vec<(String, String)> = Vec::new();
    for s in subs {
        {
            let c = storage::open_conn()?;
            storage::set_subtask_status(&c, &s.subtask_id, "run")?;
        }
        // 模拟执行耗时，让前端轮询能看到状态流转。
        tokio::time::sleep(Duration::from_millis(1200)).await;

        let node = s.assigned_node.clone().unwrap_or_else(|| "self".to_string());
        let req = ExecRequest {
            subtask_id: s.subtask_id.clone(),
            role: s.role.clone(),
            prompt: task.prompt.clone(),
            repo: task.repo.clone(),
        };

        let mut attempt = 0;
        loop {
            attempt += 1;
            match provider.execute(&req, &node) {
                Ok(out) => {
                    let v = quality::quality_gate(&out.content);
                    if v.gate_passed {
                        let c = storage::open_conn()?;
                        storage::finish_subtask(
                            &c,
                            &s.subtask_id,
                            "done",
                            &out.content,
                            v.judge_score as f64,
                            attempt,
                        )?;
                        parts.push((s.role.clone(), out.content));
                        break;
                    } else if attempt >= 2 {
                        // 质量门禁未过：不得进入聚合（SC-007）。
                        let c = storage::open_conn()?;
                        storage::finish_subtask(&c, &s.subtask_id, "failed", "", 0.0, attempt)?;
                        break;
                    }
                    // 否则重试一次（概念上重匹配，FR-009）。
                }
                Err(_) if attempt < 2 => { /* 重试 */ }
                Err(e) => {
                    tracing::warn!(subtask=%s.subtask_id, error=%e, "子任务执行失败");
                    let c = storage::open_conn()?;
                    storage::finish_subtask(&c, &s.subtask_id, "failed", "", 0.0, attempt)?;
                    break;
                }
            }
        }
    }

    let result = aggregate::aggregate(&parts);
    {
        let c = storage::open_conn()?;
        storage::set_task_result(&c, &task_id, &result)?;
        storage::set_task_state(&c, &task_id, TaskState::Aggregated)?;
    }
    tracing::info!(task = %task_id, "任务已采纳（Aggregated）");

    // 结算闭环：贡献点分发 → 哈希链账本 → 团队贡献联动（FR-010/011）。
    let settle = {
        let c = storage::open_conn()?;
        storage::settle_task(&c, &task_id, &task.title)?
    };
    {
        let c = storage::open_conn()?;
        storage::set_task_state(&c, &task_id, TaskState::Settled)?;
    }
    tracing::info!(task = %task_id, total = settle.total, nodes = settle.nodes, "任务已结算（Settled）");
    Ok(())
}

/* ---------- 阶段 9：V2 任务驱动（Mock 执行 + 结算） ---------- */

/// 驱动 compose 创建的 V2 任务：招募槽 open→claimed→working→done（MockProvider，模拟耗时）
/// → 队长审查（Mock 通过）→ 结算分配（奖金均分 / 保底）。真实领取/worktree/LLM 在阶段 10/11 替换。
pub async fn drive_v2(task_id: String) {
    if let Err(e) = drive_v2_inner(&task_id).await {
        tracing::error!(task = %task_id, error = %e, "V2 任务驱动失败");
    }
}

async fn drive_v2_inner(task_id: &str) -> anyhow::Result<()> {
    use std::time::Duration;
    let provider = MockProvider;

    let (task, self_id) = {
        let c = storage::open_conn()?;
        let t = storage::get_task(&c, task_id)?.context("任务不存在")?;
        let me = storage::ensure_node(&c)?;
        (t, me)
    };
    {
        let c = storage::open_conn()?;
        storage::set_task_state_str(&c, task_id, "executing")?;
    }

    // 本机真实 LLM 配置（无则各槽回退 Mock）
    let llm_cfg = {
        let c = storage::open_conn()?;
        storage::first_model_with_key(&c)
            .ok()
            .flatten()
            .map(|(p, m, k)| crate::llm::LlmConfig { provider: p, model: m, key: k })
    };
    // git worktree 仓库（best-effort，clone 失败回退无 worktree）
    let repo_dir: Option<(std::path::PathBuf, String)> = {
        let c = storage::open_conn()?;
        match storage::get_task_repo(&c, task_id).ok().flatten() {
            Some((kind, url, host, path, branch)) => {
                match crate::worktree::prepare_repo(task_id, &kind, &url, &host, &path, &branch) {
                    Ok(d) => Some((d, branch)),
                    Err(e) => {
                        tracing::warn!(error = %e, "worktree clone 跳过，回退无 worktree");
                        None
                    }
                }
            }
            None => None,
        }
    };

    let assignments = {
        let c = storage::open_conn()?;
        storage::list_assignments(&c, task_id)?
    };
    let members = {
        let c = storage::open_conn()?;
        storage::list_team(&c)?
    };

    for a in assignments.into_iter().filter(|a| a.status == "open") {
        let node = match_node(&a.role_name, &members).unwrap_or_else(|| self_id.clone());
        let model = members
            .iter()
            .find(|m| m.node_id == node)
            .map(|m| m.model.clone())
            .unwrap_or_else(|| "mock".to_string());
        {
            let c = storage::open_conn()?;
            storage::claim_assignment(&c, a.id, &node, &model)?;
            storage::set_assignment_status(&c, a.id, "working")?;
            storage::set_model_busy(&c, &node, &model, task_id)?;
            storage::append_op_log(&c, task_id, &node, "claim", Some(&format!("领取「{}」槽 · 模型 {model}", a.role_name)))?;
        }
        // 审查循环：执行 → 队长审查 → 不达标退回重做(≤3) → 达标采纳 / 3 次踢出重新招募
        let prompt = format!(
            "你是任务团队中的「{}」角色。任务目标：{}\n代码仓库：{}\n请直接给出你这个角色负责产出的内容（代码/测试/文档等），简洁可用。",
            a.role_name, task.prompt, task.repo
        );
        let mut attempt = 0;
        loop {
            attempt += 1;
            let started = std::time::Instant::now();
            let exec_mock = |provider: &MockProvider| {
                let out = provider
                    .execute(&ExecRequest { subtask_id: a.id.to_string(), role: a.role_name.clone(), prompt: task.prompt.clone(), repo: task.repo.clone() }, &node)
                    .unwrap();
                (out.content.clone(), out.content.chars().count() as i64)
            };
            let (content, tokens, real) = match llm_cfg.as_ref().filter(|c| crate::llm::is_real(c)) {
                Some(cfg) => match crate::llm::call_llm(cfg, &prompt).await {
                    Ok(o) => (o.content, o.tokens, true),
                    Err(e) => {
                        tracing::warn!(error = %e, "真实 LLM 调用失败，回退 Mock");
                        let (c, t) = exec_mock(&provider);
                        (c, t, false)
                    }
                },
                None => {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    let (c, t) = exec_mock(&provider);
                    (c, t, false)
                }
            };
            let work_seconds = if real { started.elapsed().as_secs().max(1) as i64 } else { (tokens / 10).max(2) };

            // 队长审查（质量门禁；角色名含「故障」者模拟始终不达标，用于演示踢出）
            let verdict = quality::quality_gate(&content);
            let accepted = verdict.gate_passed && !a.role_name.contains("故障");

            if accepted {
                let mut sha = String::new();
                if let Some((ref dir, ref branch)) = repo_dir {
                    if let Ok(s) = crate::worktree::role_worktree(dir, branch, &a.role_name, a.slot_index).and_then(|wt| {
                        let s = crate::worktree::commit_output(&wt, &a.role_name, &content)?;
                        let c = storage::open_conn()?;
                        storage::set_assignment_worktree(&c, a.id, &wt.to_string_lossy())?;
                        Ok(s)
                    }) {
                        sha = s;
                    }
                }
                let c = storage::open_conn()?;
                storage::finish_assignment(&c, a.id, tokens)?;
                storage::set_model_idle(&c, &node, &model, tokens, work_seconds)?;
                let tag = if real { "真实LLM" } else { "Mock" };
                let extra = if sha.is_empty() { String::new() } else { format!(" · commit {sha}") };
                storage::append_op_log(&c, task_id, &self_id, "review", Some(&format!("审查通过「{}」(第 {attempt} 次)", a.role_name)))?;
                storage::append_op_log(&c, task_id, &node, "submit", Some(&format!("完成「{}」({tag}) · {tokens} tokens · {work_seconds}s{extra}", a.role_name)))?;
                break;
            }

            // 不达标 → 退回重做
            {
                let c = storage::open_conn()?;
                storage::append_op_log(&c, task_id, &self_id, "reject", Some(&format!("审查退回「{}」(第 {attempt} 次未达标)", a.role_name)))?;
            }
            if attempt >= 3 {
                // 重试 3 次仍不达标 → 踢出该角色，槽位回市场重新招募（需求 10）
                let c = storage::open_conn()?;
                storage::reopen_assignment(&c, a.id)?;
                storage::set_model_idle(&c, &node, &model, 0, 0)?;
                storage::append_op_log(&c, task_id, &self_id, "kick", Some(&format!("踢出「{}」(3 次未达标) · 槽位回市场重新招募", a.role_name)))?;
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await; // 重做间隔
        }
    }

    // 队长审查（Mock 通过）
    {
        let c = storage::open_conn()?;
        storage::set_task_state_str(&c, task_id, "reviewing")?;
        storage::append_op_log(&c, task_id, &self_id, "review", Some("队长审查：各角色产出达标"))?;
        storage::set_task_state_str(&c, task_id, "aggregated")?;
    }
    // 结算分配
    let s = {
        let c = storage::open_conn()?;
        storage::settle_task_v2(&c, task_id)?
    };
    tracing::info!(task = %task_id, total = s.total, nodes = s.nodes, bonus = s.bonus, "V2 任务已结算");
    Ok(())
}
