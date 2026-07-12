# Contract: Coding Agent

Applies to Phase 2 execution on a `claimed`/`working` assignment.

## Goal

Run a tool loop until one of: role goal satisfied, step limit, token limit, or hard error. Persist an `AgentRun` with tool trace summary.

## Tool surface (MUST)

| Tool | Allowed | Notes |
|------|---------|-------|
| `list_dir` | worktree root only | |
| `read_file` | under worktree | reject `..` escape |
| `write_file` / `apply_patch` | under worktree | |
| `run_command` | cwd = worktree; timeout + output cap | |
| `git_status` / `git_diff` / `git_commit` | worktree repo | |

## Forbidden (MUST reject)

- Paths outside assignment worktree
- Mutating captain/main working tree used by other slots without explicit policy
- Exfiltrating model API keys into repo or logs

## Lifecycle hooks

1. `start` → create/attach worktree (`<repo>/.worktrees/<role>-<slot>`)
2. loop tools ↔ model
3. `submit` → commit + assignment `submitted` + op_log
4. captain `review` → accept/reject/kick per assignment state machine

## Limits

- Configurable `max_steps`, `max_tokens`, per-command timeout
- On limit: status `limited` / failed per policy; never mark `done` without review accept

## Providers

Real HTTP providers (OpenAI-compat / Anthropic / Ollama / MiniMax) when configured; Mock allowed for tests. Phase 2 “real collab” acceptance requires at least one real provider path.
