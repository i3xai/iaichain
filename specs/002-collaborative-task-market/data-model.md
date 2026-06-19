# 数据模型与规则细化（V2，动手前评审稿）

> 配套 [`design.md`](./design.md)。决策已定：协调中继 / A 类先 Mock / 先评审。
> 标 **⚠ 待确认** 的是需你拍板的规则歧义点（见文末汇总）。

## 1. 表结构（迁移 v7+，当前 v6）

### 1.1 task（扩展现有表）
| 列 | 类型 | 说明 |
|---|---|---|
| repo_kind | TEXT | `opensource` \| `internal` |
| repo_url | TEXT | 开源 git 地址 |
| server_host | TEXT | 内部服务器 host |
| server_path | TEXT | 内部 git 目录（须为 git 仓库）|
| branch | TEXT | 留空→自动建 `task/<task_id>` |
| reward_total | INTEGER | 奖励金（贡献币），0=仅保底 |
| reward_locked | INTEGER | 发起时锁定额度 |
| captain_node | TEXT | 队长节点 |
| visibility | TEXT | `private` \| `network` |
| archived_at | TEXT | 归档时间（队长归档后置）|

### 1.2 role_template（节点角色库，可复用，需求 3）
`id, node_id, name, prompt, is_captain(0/1), model_filter, created_at`
- 队长模板（is_captain=1）内置、不可删，携带默认驱动提示词。
- 其余角色可增删、无上限。

### 1.3 task_role（任务的角色配置，需求 3/4/6）
`id, task_id, name, prompt, recruit_count(≥1), model_filter('any'|csv), is_captain(0/1)`
- 创建任务时至少 1 个非队长角色（need ≥ 1）。

### 1.4 assignment（招募槽位 = 角色实例，需求 4/7/8/10）
`id, task_id, task_role_id, slot_index, node_id, model, worktree_path,
 status, attempts, tokens, started_at, ended_at`
- 每个 task_role 生成 `recruit_count` 个槽（slot_index 0..N-1）。
- **状态机**：
```
open ──claim──► claimed ──start──► working ──submit──► submitted ──accept──► done
  ▲                                  │                      │
  └──────── kick(超时3次/审查弃用) ───┴──────reject(退回)────┘
```
  - `open`：可被网络领取。`claimed`：已领取待启动。`working`：worktree 内执行中。
  - `submitted`：产出待队长审查。`done`：审查通过。
  - `reject`：队长退回 → 回 `working` 重做。`kick`：踢出 → 回 `open` 重新招募。

### 1.5 model_instance（节点模型工作态，需求 8）
`node_id, model, status('idle'|'busy'), current_task, tokens_used, work_seconds, updated_at`
- **唯一约束** `(node_id, model)`；busy 时 current_task 唯一 → 单模型同时仅一个任务。

### 1.6 op_log（任务操作日志，需求 12）
`id, task_id, ts, actor, action, detail(JSON)`
- action 枚举：`create|repo_check|claim|start|llm_call|submit|review|reject|accept|kick|recruit|settle|archive`。

### 1.7 reward_alloc（结算分配，需求 5/11）
`id, task_id, node_id, role, credits, basis, ts`

### 1.8 relay 侧（中继服务器，需求 7）
公告板可直接复用 task 表的 `visibility='network'` 行 + assignment 的 `open` 槽；
中继提供：节点注册、任务公告查询、领取协商（防重复领取的乐观锁/原子占位）。

## 2. task 生命周期（扩展 V1 七态）
```
Created → Parsed → Decomposed(角色配置) → Recruiting(招募中) →
Executing(执行) → Reviewing(队长审查) → Aggregated(达标) → Settled(结算归档)
```
- `Recruiting`：有 open 槽，等网络领取/匹配。
- `Reviewing`：队长对 submitted 审查，可 reject 回退。
- `Settled`：奖金平分 + 保底发放 + 贡献分分配 + 归档。

## 3. 经济规则（需求 5/11）
- 创建校验：`reward_total ≤ 本机可用贡献币`，否则拒绝创建。
- 发起成功：`ledger Lock`（amount=-reward_total, locked_delta=+reward_total）锁定。
- 完成结算：
  - 配了奖金 → 在**完成（done）的角色节点**间**均分** `reward_total`（`Unlock` 释放锁定 + 各 `Settle` 入账）。
  - 未配奖金 → 每个 done 角色节点**保底 +1 贡献币**（`Settle`）。
- 队长（不开发）作为发起/受益方**不参与奖金分配**；但负责在 `reward_alloc` 记录贡献分。

## 4. 关键流程（时序）
1. **创建**：填仓库(单选开源/内部)+分支 → 连通性检测 → 配角色(队长内置+≥1其他,各设提示词/招募数/模型筛选) → 配奖金(校验余额) → 提交：建 task + task_role + assignment(open 槽) + Lock 奖金 + op_log(create)。
2. **连通性检测** `POST /api/repo/check`：开源→`git ls-remote <url> [branch]`；内部→ssh `git -C <path> rev-parse --is-inside-work-tree` + 列分支。超时/失败→禁止创建。
3. **领取/匹配**：网络节点查 open 槽（按角色/模型/奖金过滤）→ claim（中继原子占位防重复）→ 校验模型匹配 → claimed。一键自动匹配=选奖金最高的可匹配 open 槽；托管=空闲轮询自动 claim。
4. **执行(阶段11)**：claimed→worktree(clone/检出分支)→LLM agent 开发→submit。
5. **审查(阶段12)**：队长对照目标审查 submitted；达标 accept→done；否则 reject 回 working。
6. **超时踢出(阶段12)**：working 槽心跳；3 次重试无响应→kick→worktree 回收→槽回 open 重新招募。
7. **结算归档**：全部 done→平分/保底→reward_alloc→archive→op_log(settle/archive)。

## 5. ✅ 已确认规则（2026-06-19 拍板）
1. **奖金分配**：配奖金 → 在 done 角色节点间**均分**；队长不分；配奖金则**不叠加**保底。未配 → 每个 done 角色节点**保底 +1 贡献币**。
2. **货币统一**：「贡献分」即「贡献币」，单一货币；奖金分配即贡献分分配。
3. **内部远程仓库认证**：以**节点本机 ssh 免密可达** `server_host` 为前提，不在任务中存储凭证。
4. **踢出补位优先级**：先用**排队中的超额领取者**补位；无排队再回市场公开招募。
5. **Mock 阶段多节点演示**：用**单机多实例**（不同 `IAI_HOME` + 端口）连同一中继演示领取/匹配。
6. **命名规范**：自动分支 `task/<task_id>`；worktree 路径 `<repo>/.worktrees/<role>-<slot>`。
