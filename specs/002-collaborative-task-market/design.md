# 可行性分析与设计：协作式任务市场 V2

> 目标：把 V1（MockProvider + 手动注册表 + 轮询的 demo）升级为**真实的分布式 AI 开发协作市场**。
> 本文覆盖 12 条需求的可行性、最优设计、数据模型与分阶段开发计划。

## 0. 总评

整体**可行**。把 12 条需求按「与当前架构的距离」分两类：

| 类 | 需求 | 性质 | 工作量 |
|---|---|---|---|
| **A 类**（现有架构直接扩展） | 1 任务弹框、2 仓库配置/连通性、3 自定义角色、4 招募数量、5 奖励金、6 模型筛选、12 操作日志 | 数据模型 + UI + 经济扩展 | 中 |
| **B 类**（需根本能力升级） | 7 网络领取/自动匹配/托管、8 多模型并发与 token/时长、9 队长汇总、10 超时踢出补位、11 队长 AI 驱动审查 | 真实 LLM agent + 真实节点网络 + git worktree | 大 |

A 类可立即在 V1 之上落地（执行仍可用 Mock 占位）；B 类依赖三项前置升级（见 §1）。

---

## 1. 三项前置架构升级（B 类的共同依赖）

### 1.1 真实 LLM Agent 执行（替代 MockProvider）
需求 8（token/时长）、9（工作历史）、11（审查驱动）都要求**真实模型在 git 仓库里干活**。
- 每个角色节点 = 一个在 **git worktree 内的受限 AI agent**：读写文件 / 跑命令 / git 提交，由 LLM 驱动。
- `ModelProvider` trait 已预留；需补：HTTP 适配（OpenAI/Anthropic/Ollama，带 usage 统计）+ agent 工具循环。
- token/时长：每次 Provider 调用记录 `usage`（prompt/completion tokens）与耗时，落 `model_instance`。

### 1.2 真实节点间网络（替代手动 invite + 占位 P2P）
需求 7 要求任务对网络可见、其他节点领取/匹配/托管。
- **推荐：轻量协调中继（任务公告板 relay）** —— 节点注册、任务公告、领取协商走中继；记账仍各节点本地自治。简单可靠，可立刻支撑领取/自动匹配/托管。
- 备选：纯 P2P（mDNS 局域网 + DHT/gossip 跨网 + NAT 穿透）—— 符合「无中心」宣言但复杂，列为后续。
- 这台已部署的服务器（139.224.28.252）天然可充当中继。

### 1.3 实时通道（SSE）
需求 8（模型状态实时）、10（心跳/超时）、12（操作日志流）。当前为前端轮询 → 升级 SSE 推送。

> 结论：**A 类先行**（无需前置，立即提升「好用」）；**B 类按 1.1→1.2→1.3 顺序补能力**。

---

## 2. 数据模型扩展（迁移 v7+，当前 v6）

```text
task（扩展列）
  + repo_kind        TEXT   -- 'opensource' | 'internal'
  + repo_url         TEXT   -- 开源 git 地址（opensource）
  + server_host      TEXT   -- 内部服务器（internal）
  + server_path      TEXT   -- 内部 git 目录（internal）
  + branch           TEXT   -- 留空→自动创建任务分支 task/<id>
  + reward_total     INTEGER DEFAULT 0  -- 奖励金（贡献币），0=仅保底
  + reward_locked    INTEGER DEFAULT 0  -- 发起时锁定的额度
  + captain_node     TEXT   -- 队长节点
  + visibility       TEXT   -- 'private' | 'network'

role_template（节点级可复用角色库，需求 3）
  id, node_id, name, prompt, is_captain(0/1), model_filter, created_at
  -- 队长模板默认存在且不可删；其余可增删，无上限

task_role（任务的角色配置，需求 3/4/6）
  id, task_id, name, prompt, recruit_count(≥1), model_filter('any'|...),
  is_captain(0/1)

assignment（角色实例 = 招募槽位，需求 4/7/8/10）
  id, task_id, task_role_id, slot_index,
  node_id(null=未领取), model, worktree_path,
  status('open'|'claimed'|'working'|'submitted'|'done'|'failed'|'kicked'),
  attempts, tokens, started_at, ended_at
  -- recruit_count 个槽位；多余领取者排队；踢出→置 open 重新招募

model_instance（节点模型工作态，需求 8）
  node_id, model, status('idle'|'busy'), current_task,
  tokens_used, work_seconds, updated_at
  -- 约束：单 (node_id, model) 同时仅 current_task 一个

op_log（任务操作日志，需求 12）
  id, task_id, ts, actor, action, detail
  -- AI 每步、心跳、踢出、审查、贡献分分配全记录

reward_alloc（结算分配，需求 5/11）
  id, task_id, node_id, role, credits, basis, ts
```

`assignment` 取代 V1 的 `subtask`（subtask 保留兼容或迁移）。`ledger` 复用现有 `Lock`/`Unlock`/`Settle`/`Reward` 实现奖励金锁定与平分。

---

## 3. 逐需求设计

| # | 需求 | 设计要点 | 类 |
|---|---|---|---|
| 1 | 任务独立页/弹框 | 新建 `/console` 任务创建弹框（多步表单）；落地 `task` 扩展列 | A |
| 2 | 仓库配置 + 连通性 | 单选切换 开源 vs 内部；`POST /api/repo/check` 用 `git ls-remote`（开源）或 ssh+`git -C <dir> rev-parse`（内部）探活；不通禁止创建；空分支→建 `task/<id>`；成员各开 `git worktree` | A(检测)+B(worktree) |
| 3 | 自定义角色 + 提示词 | `role_template` CRUD；队长模板内置不可删（默认驱动提示词）；其他≥1、无上限 | A |
| 4 | 招募数量 + 补位 + 并行 | `task_role.recruit_count` 生成 N 个 `assignment` 槽；超额领取排队；踢出置 open 自动补位；同角色多槽并行，**各自独立 worktree** | A(模型)+B(执行) |
| 5 | 奖励金 | 创建时校验 `reward_total ≤ 本机可用贡献币`；发起成功 `Lock` 锁定；完成后在参与角色节点间**平分**（`Settle`）；未配则每个完成角色**保底 +1 币** | A |
| 6 | 角色模型筛选 | `task_role.model_filter`（默认 `any`）；领取时校验领取节点模型匹配 | A |
| 7 | 网络可见 + 领取 + 自动/托管匹配 | 中继公告板 `GET /api/network/tasks`（按未占角色+模型+奖金过滤）；`POST /api/tasks/:id/claim`；**一键自动匹配**（手动触发，选奖金最高的匹配槽）；**托管匹配**（节点开关，空闲轮询自动领取） | B |
| 8 | 多模型并发 + 监控 | `model_instance` 表 + 单模型单任务约束；控制台「模型」面板：状态/累计 token/工作时长；SSE 实时 | B |
| 9 | 汇总到队长 + 工作历史 | 各 `assignment` 产出汇总到队长节点；队长「工作历史」视图查所有模型记录（`op_log` + `model_instance`） | B |
| 10 | 超时踢出 + 重新招募 | 队长对 working 槽心跳；3 次重试无响应→`status=kicked`、释放 worktree、槽位置 `open` 回市场 | B |
| 11 | 队长 AI 驱动 + 审查归档 | 队长不开发：派发→收集→对照目标审查→不达标退回对应角色重做→全部达标后汇报+归档+`reward_alloc` 分配贡献分；全程 `op_log` | B |
| 12 | 操作日志 | 所有 AI 步骤/心跳/踢出/审查/分配写 `op_log`（task 维度），历史任务可查 | A(框架)+B(内容) |

---

## 4. 分阶段开发计划（延续 V1 阶段编号，从阶段 8 起）

> 每阶段可独立交付、可验证、可衔接，延续 V1「接缝翻转 + 浏览器实测 + 测试覆盖」节奏。

- **阶段 8 · 任务创建与角色配置（A 类核心，最快见效）**
  任务创建弹框（独立多步表单）；仓库双模式单选 + `POST /api/repo/check` 连通性检测；`role_template` CRUD（队长内置不可删）；`task_role` 招募数量 + 模型筛选；`task` 扩展列落库。执行仍 Mock。
- **阶段 9 · 奖励金经济 + 操作日志 + 归档（A 类）**
  奖励金校验/锁定/平分/保底（复用 ledger Lock/Settle）；`op_log` 全链路；队长归档 + `reward_alloc` 贡献分；历史任务详情视图。
- **阶段 10 · 节点网络与领取（B 类·网络）**
  协调中继公告板；网络任务列表（按未占角色/模型/奖金过滤）；领取 + 一键自动匹配 + 托管匹配；`model_instance` 单模型单任务约束 + 控制台模型面板（SSE）。
- **阶段 11 · 真实 LLM Agent + git worktree（B 类·执行）**
  Provider HTTP 适配（usage 统计）；节点 agent 在 worktree 内 clone/编辑/commit；自动建任务分支；token/时长/状态追踪。
- **阶段 12 · 可靠性与队长审查闭环（B 类·编排）**
  心跳/超时/3 次重试/踢出/自动补位/重新招募；队长 AI 审查-退回-达标-汇报-归档循环；全程 op_log。

---

## 5. 决策记录（已拍板）

1. **网络形态**：✅ **协调中继服务器**（已部署的 139.224.28.252 充当任务公告板/中继）；记账各节点本地自治。纯 P2P 列为后续。
2. **真实执行**：✅ **A 类先用确定性 MockProvider 跑通**全流程（创建/角色/招募/经济/领取）；真实 LLM agent + git worktree 放**阶段 11**。
3. **本轮动作**：✅ **先评审细化设计再动手** —— 故本目录补 [`data-model.md`](./data-model.md)（数据模型/状态机/经济规则）；规则歧义点在该文件以「⚠ 待确认」标注，对齐后再写 API 契约并进入阶段 8。
4. **worktree 执行环境**（阶段 11 再定）：倾向节点直接操作本地 git worktree；容器隔离作为加固后续。

---

## 6. 与现状的衔接
- 复用：`ledger`（Lock/Unlock/Settle/Reward）、`team_member`、`auth_sessions`（节点鉴权）、`ModelProvider` trait、SSE 升级点。
- 兼容：`subtask` → `assignment` 迁移；V1 的 `getTasks`/`createTask` 接缝平滑扩展。
