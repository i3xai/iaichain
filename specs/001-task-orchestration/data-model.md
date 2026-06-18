# Phase 1 Data Model: IAI 核心任务编排与结算闭环

实体源自 spec「Key Entities」与功能需求，落为 Rust 类型 + SQLite 表。所有时间为 UTC+8。

## 实体

### Task（任务）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| task_id | String (UUID) | 主键，唯一 |
| description | String | 原始自然语言描述（FR-001） |
| state | TaskState enum | 七态之一（FR-002） |
| subtask_ids | Vec<String> | 关联子任务 |
| aggregated_result_id | Option<String> | 最终聚合结果引用 |
| created_at | DateTime | UTC+8 |

### Subtask（子任务）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| subtask_id | String (UUID) | 主键 |
| task_id | String | 外键 → Task |
| required_capability | Capability | 单一能力（FR-003） |
| assigned_node_id | Option<String> | 匹配的节点（FR-004） |
| result_id | Option<String> | 执行结果引用 |
| quality_passed | bool | 是否通过质量门禁（FR-008） |
| attempts | u32 | 重试次数（FR-009） |

### IAI Node（节点）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| node_id | String | 主键，唯一（FR-005） |
| capabilities | Vec<Capability> | 声明能力列表 |
| models | Vec<String> | 可用模型列表 |
| status | NodeStatus enum | Available / Busy / Offline |
| total_credits | u64 | 累计贡献点 |
| success_rate | f32 | 历史成功率（用于匹配评分，FR-004） |

### Ledger Entry（账本记录，append-only 哈希链）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| seq | u64 | 自增序号，主键 |
| task_id / subtask_id | String | 关联执行 |
| node_id | String | 参与节点 |
| credits | u64 | 贡献点（FR-011） |
| reward | u64 | 奖励分发 |
| evidence_hash | String | 执行证据指纹（FR-010/016） |
| prev_hash | String | 前序记录哈希 |
| entry_hash | String | 本记录哈希 = sha256(规范化字段 + prev_hash) |
| created_at | DateTime | UTC+8 |

> 不可更新/删除；`verify` 通过重算 entry_hash 链校验完整性（FR-013, SC-004）。

### Market Price（市场价格）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| capability | Capability | 能力类型 |
| price | u64 | 当前定价（FR-012） |
| supply_demand_basis | String | 公开供需依据（可复算，SC-006） |
| published_at | DateTime | 公示时间 |

### Result（结果）
| 字段 | 类型 | 约束 / 说明 |
|------|------|------------|
| result_id | String (UUID) | 主键 |
| source_node_id | String | 来源节点 |
| content | String | 交付内容 |
| quality_score | f32 | 裁判模型评分（FR-008） |
| gate_passed | bool | 是否达阈值 |
| candidates | Vec<(node_id, score)> | 冲突候选来源及评分（Q2，如适用） |

## 枚举

- `Capability`: `Reasoning | Coding | Writing | ...`（可扩展）
- `TaskState`: `Created → Parsed → Decomposed → Matched → Executed → Aggregated → Settled`
- `NodeStatus`: `Available | Busy | Offline`

## 状态机（Task，单向）

```text
Created → Parsed → Decomposed → Matched → Executed → Aggregated → Settled
```

转移规则：
- 仅允许相邻向前转移；非法跳转由转移函数拒绝（FR-002）。
- `Executed → Aggregated` 前各子任务结果须存在；`Aggregated → Settled` 前须
  (a) 通过质量门禁（FR-008）且 (b) 已写入账本记录（FR-010）。
- 任一子任务节点失败/超时 → 该子任务回到 Matched 重新匹配（attempts+1），不回退整任务（FR-009）。
- 每次转移写一条审计记录：{from, to, input_digest, node_id, result_fingerprint, ts}（FR-016）。

## 校验规则摘要

- node_id 全局唯一；注册时校验（FR-005）。
- 无账本记录的执行不得进入 Settled（FR-010）。
- quality_passed=false 的结果不得进入聚合/结算（FR-008, SC-007）。
- 账本链 entry_hash 必须可重算一致（FR-013, SC-004）。
