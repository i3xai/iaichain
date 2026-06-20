//! 任务 git worktree（阶段 11）：clone 仓库 → 任务分支 → 每角色独立 worktree → 提交产出。
//!
//! best-effort：任何 git 步骤失败都不阻塞任务（回退为「无 worktree」，仅记录）。
//! 工作区在 `$IAI_HOME/work/<task_id>`。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

use crate::storage;

fn work_root() -> PathBuf {
    storage::data_dir().join("work")
}

/// 准备任务仓库：浅 clone + 创建任务分支。返回 repo 目录。
pub fn prepare_repo(
    task_id: &str,
    repo_kind: &str,
    repo_url: &str,
    host: &str,
    path: &str,
    branch: &str,
) -> anyhow::Result<PathBuf> {
    let root = work_root();
    let dir = root.join(task_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    std::fs::create_dir_all(&root)?;
    let clone_src = if repo_kind == "internal" {
        format!("ssh://{host}/{}", path.trim_start_matches('/'))
    } else {
        repo_url.to_string()
    };
    run_git(&root, &["clone", "--depth", "1", &clone_src, task_id])
        .with_context(|| format!("clone 失败: {clone_src}"))?;
    // 任务分支（基于默认分支）
    run_git(&dir, &["checkout", "-B", branch]).ok();
    Ok(dir)
}

/// 为某角色开 worktree（独立分支 `<branch>-<role>-<slot>`）。返回 worktree 路径。
pub fn role_worktree(repo_dir: &Path, branch: &str, role: &str, slot: i64) -> anyhow::Result<PathBuf> {
    let wt = repo_dir.join(".worktrees").join(format!("{role}-{slot}"));
    let wt_str = wt.to_string_lossy().to_string();
    let wt_branch = format!("{branch}-{role}-{slot}");
    if wt.exists() {
        run_git(repo_dir, &["worktree", "remove", "--force", &wt_str]).ok();
    }
    run_git(repo_dir, &["worktree", "add", "-B", &wt_branch, &wt_str, branch])
        .with_context(|| "worktree add 失败")?;
    Ok(wt)
}

/// 在 worktree 写入角色产出并提交。返回 commit 短哈希。
pub fn commit_output(wt: &Path, role: &str, content: &str) -> anyhow::Result<String> {
    let file = wt.join(format!("IAI-{role}.md"));
    std::fs::write(&file, content)?;
    run_git(wt, &["add", "."])?;
    run_git(
        wt,
        &[
            "-c", "user.email=node@iai.chain",
            "-c", "user.name=IAI Node",
            "commit", "-m", &format!("[{role}] AI 产出"),
        ],
    )?;
    let sha = run_git(wt, &["rev-parse", "--short", "HEAD"])?.trim().to_string();
    Ok(sha)
}

fn run_git(cwd: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .context("执行 git 失败")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} 失败: {}",
            args,
            String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
