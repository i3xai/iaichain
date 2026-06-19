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
