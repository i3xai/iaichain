# API 契约与中继协议（V2）

> 配套 [`data-model.md`](./data-model.md)。规则已定稿。本文是动手前最后的接口契约。
> 阶段 8 仅需「本节点 API」中标 **[8]** 的端点；标 **[10]** 的属阶段 10（中继/网络）。

## 1. 本节点 API（127.0.0.1:8787，经 nginx 对外）

### 角色库（role_template，需求 3）
| 方法 路径 | 请求 | 响应 | 阶段 |
|---|---|---|---|
| GET `/api/roles` | — | `[{id,name,prompt,isCaptain,modelFilter}]`（队长在前）| [8] |
| POST `/api/roles` | `{name,prompt,modelFilter?}` | `{ok,id}` | [8] |
| PUT `/api/roles/:id` | `{name?,prompt?,modelFilter?}` | `{ok}`（队长仅可改 prompt）| [8] |
| DELETE `/api/roles/:id` | — | `{ok}` / 400 队长不可删 | [8] |

### 仓库连通性检测（需求 2）
| 方法 路径 | 请求 | 响应 | 阶段 |
|---|---|---|---|
| POST `/api/repo/check` | `{kind:'opensource',url,branch?}` 或 `{kind:'internal',host,path,branch?}` | `{ok:true,branches:[...]}` 或 `{ok:false,error}` | [8] |

- opensource：`git ls-remote <url>`（5s 超时）；给了 branch 则校验存在。
- internal：ssh `git -C <path> rev-parse --is-inside-work-tree` + `git -C <path> branch --format='%(refname:short)'`。

### 任务创建（需求 1–6）
| 方法 路径 | 请求 | 响应 | 阶段 |
|---|---|---|---|
| POST `/api/tasks` | 见下 TaskCreate | `{ok,taskId}` / 400（不通/余额不足/角色缺失）| [8] |
| GET `/api/tasks` | — | 任务卡数组（扩展 V1：含 roles/slots/reward/state）| [8] |
| GET `/api/tasks/:id` | — | 详情：task + task_role + assignment + reward_alloc | [8] |
| GET `/api/tasks/:id/log` | — | `op_log` 时间线 | [9] |

```jsonc
// TaskCreate 请求体
{
  "title": "实现限流中间件",
  "repo": { "kind": "opensource", "url": "https://github.com/acme/gw", "branch": "" },
  // 或 { "kind":"internal", "host":"10.0.0.5", "path":"/srv/gw", "branch":"" }
  "roles": [
    { "name":"后端", "prompt":"...", "recruitCount":2, "modelFilter":"any" },
    { "name":"测试", "prompt":"...", "recruitCount":1, "modelFilter":"claude-3-5-sonnet" }
  ],
  "reward": 0,            // 贡献币；0=仅保底
  "visibility": "network" // private | network
}
```
创建校验：仓库连通 ✓、非队长角色 ≥1、各 recruitCount ≥1、`reward ≤ 可用贡献币`。
成功：建 task + task_role + N 个 open assignment + `Lock` 锁定 reward + op_log(create)。

### 模型工作态（需求 8）
| 方法 路径 | 请求 | 响应 | 阶段 |
|---|---|---|---|
| GET `/api/models/instances` | — | `[{model,status,currentTask,tokensUsed,workSeconds}]` | [10] |

### 领取与匹配（需求 7）
| 方法 路径 | 请求 | 响应 | 阶段 |
|---|---|---|---|
| GET `/api/network/tasks` | 查询 open 槽（过滤 role/model/minReward）| 网络任务数组 | [10] |
| POST `/api/assignments/:id/claim` | `{model}` | `{ok}` / 409 已被占 | [10] |
| POST `/api/match/auto` | — | 选奖金最高的可匹配槽并领取 | [10] |
| PUT `/api/match/hosted` | `{enabled}` | 托管开关（空闲自动领取）| [10] |

## 2. 中继协议（阶段 10，139.224.28.252）

节点 ↔ 中继，最小公告板 + 原子领取：
| 方法 路径 | 说明 |
|---|---|
| POST `/relay/register` | `{nodeId,endpoint,models[],capabilities[]}` 节点注册/心跳 |
| POST `/relay/publish` | 队长发布网络任务（task + open 槽摘要）|
| GET `/relay/tasks` | 拉取公告板（其他节点据此 GET 详情/领取）|
| POST `/relay/claim` | `{assignmentId,nodeId,model}` 原子占位（乐观锁，防重复领取），返回成功/冲突 |
| POST `/relay/heartbeat` | 角色节点对进行中槽的心跳（队长据此判超时，需求 10）|

记账不经中继：各节点本地 ledger 自治；中继只做发现/协商/心跳转发。

## 3. 阶段 8 落地范围（本轮编码起点）
- 迁移 v7：task 扩展 + role_template + task_role + assignment + op_log（model_instance/reward_alloc/relay 属 9/10）。
- API：`/api/roles*`、`/api/repo/check`、扩展 `POST /api/tasks`、`GET /api/tasks(/:id)`。
- 前端：任务创建弹框（仓库单选+检测、角色编辑器、招募数/模型筛选、奖金+余额校验）。
- 执行仍 MockProvider；网络/领取/真实 worktree 见阶段 10/11。
