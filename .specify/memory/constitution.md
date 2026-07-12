<!--
SYNC IMPACT REPORT
==================
Version change: 1.0.0 → 1.1.0
Bump rationale: Align constitution with product reality and 003 open collab market —
  extended lifecycle states, coordination relay allowed, anti-fraud baseline,
  agent execution constraint.

Modified principles:
  - II. 任务生命周期完整性 — allow Recruiting/Reviewing with mapping to baseline seven states
  - III. 去中心化与可调度 — clarify coordination relay ≠ settlement SPOF; pure P2P deferred
  - V. 自由市场与公平定价 — add anti-fraud baseline MUST
  - VI. 可观测与质量验证 — allow rule → model-as-judge / captain review progression
Added sections:
  - Agent 执行约束 (under Architectural Constraints)
Templates: no template path changes required.

Follow-up: specs/003-open-collab-market is current source of truth (see specs/STATUS.md).
-->

# IAI Constitution

去中心化 AI 能力与任务市场系统（IAI）的根本治理章程。本章程是项目最高约束，
所有规格、计划、任务与实现均须遵守；冲突时以本章程为准。

## Core Principles

### I. 节点能力标准化 (Node Capability Standardization)

每一项 AI 能力 MUST 被封装为标准化的 IAI Node（AI 能力容器），并显式声明其能力契约：

- 每个节点 MUST 提供唯一 `node_id`、`capabilities` 列表与 `models` 列表，作为可被
  发现与调度的契约。
- 节点 MUST 自包含、可独立部署、可独立测试；禁止依赖隐式全局状态。
- 调度层 MUST 仅通过声明的能力契约与节点交互，禁止绕过契约直接耦合具体模型实现。

**Rationale**：标准化容器是"分散能力 → 可运行节点"的前提，决定了网络的可组合性与可替换性。

### II. 任务生命周期完整性 (Task Lifecycle Integrity)

每个 Task MUST 完整经历既定生命周期，且状态转移可审计：

- 基线生命周期状态 MUST 为：Created → Parsed → Decomposed → Matched → Executed →
  Aggregated → Settled。
- 产品路径 MAY 使用扩展态（如 Recruiting、Reviewing），但 MUST 在规格中提供与基线态的
  映射，且 MUST NOT 跳过 Aggregated/Settled 直接结束。
- 状态转移 MUST 被记录；每次转移 MUST 留存可追溯记录（输入、负责节点、结果指纹），
  供事后审计与结算。

**Rationale**：任务是系统的核心工作单元，可审计的生命周期是协作信任与结算正确性的基础。

### III. 去中心化与可调度 (Decentralization & Schedulability)

核心闭环 Task → Orchestrator → Node Network → Execution → Result → Ledger →
Market MUST 在无单点故障前提下运行：

- Orchestrator MUST 将节点视为可发现、可替换、可容错的资源；单节点失败 MUST 不导致
  任务整体失败（须支持重试或重新匹配）。
- 系统 MUST NOT 引入对**结算与账本正确性**具备否决权的中心化组件。
- 允许使用**协调中继**仅承担发现、任务公告、领取占位与心跳转发；记账与结算 MUST
  留在参与节点可核验的账本路径上。纯 P2P 发现列为后续演进，不作为当前合规前提。
- 调度决策 MUST 基于能力匹配与公开的市场信号，而非硬编码偏好。

**Rationale**：去中心化是产品定义的核心承诺；可调度性确保算力协作在规模下仍然可靠。
协调与结算分离，避免把实用中继误写成结算中心。

### IV. 经济可结算与可追溯 (Economic Settlement & Traceability)

所有经济行为 MUST 可结算、可追溯、防篡改：

- 每次 Execution MUST 产生对应的 Ledger（账本）记录；无账本记录的执行 MUST NOT 进入
  Settled 状态。
- Contribution Credit（贡献点）与 Reward Distribution（奖励分发）MUST 可由第三方独立
  核验，且账本 MUST 防篡改（append-only / 可验证）。
- 结算 MUST 与任务生命周期记录一一对应，禁止脱离执行证据的奖励发放。

**Rationale**：价值结算是激励体系存续的根基，缺乏可追溯性将瓦解贡献者信任。

### V. 自由市场与公平定价 (Free Market & Fair Pricing)

能力与任务的定价 MUST 由自由市场决定且公开透明：

- 定价 MUST 基于公开的供需信号（Free Market Pricing），禁止隐藏费用或暗箱加价。
- 价格信号 MUST 对参与方对称可见；MUST NOT 对特定节点提供未公示的优待。
- 市场规则的任何变更 MUST 公示并适用于全体参与者。
- 面向开放网络时，系统 MUST 具备至少一种**防刷基线**（身份绑定、领取押金或信誉门槛等），
  且规则公开、可复算。

**Rationale**：公平、透明的市场是吸引并留存 AI 能力供给方的前提；开放参与需要最低作弊成本。

### VI. 可观测与质量验证 (Observability & Quality Verification)

结果在结算前 MUST 经过质量验证，系统全程 MUST 可观测：

- Aggregated 阶段 MUST 对节点结果执行可验证的聚合与质量门禁；未通过质量门禁的结果
  MUST NOT 进入 Settled。
- 质量门禁实现 MAY 从确定性规则演进到裁判模型或队长 agent 审查，但「未通过不得结算」
  的行为契约 MUST 保持不变。
- 关键路径（解析、分解、匹配、执行、聚合、结算）MUST 输出结构化日志与指标。
- 多模型协作产出 MUST 可复核来源与贡献，以支撑争议处理与质量追责。

**Rationale**：质量验证保护任务发起方利益；可观测性使去中心化网络的故障可定位、可改进。

## 分层架构约束 (Architectural Constraints)

- 系统 MUST 维持四层分层：应用层（Client Layer）、核心调度层（IAI Core Layer）、
  节点运行层（IAI Node Layer）、经济系统层（Economic Layer）。
- 跨层调用 MUST 单向自上而下依赖；下层 MUST NOT 直接依赖上层实现细节。
- 节点运行层与经济系统层 MUST 通过核心调度层交互，禁止应用层直接驱动经济结算。
- 任何新增组件 MUST 明确归属某一层，并遵循该层的契约边界。

### Agent 执行约束

- 节点侧执行（含 coding agent）MUST 通过声明的工具/能力契约进行，禁止未声明的隐式副作用。
- 开放网络下 MUST 具备隔离边界（至少 assignment worktree 路径约束；更强沙箱为演进项）。

## 开发工作流与质量门禁 (Development Workflow & Quality Gates)

- 所有特性 MUST 遵循 Spec-Kit 流程：specify → plan → tasks → implement；计划阶段
  MUST 通过 Constitution Check 门禁。
- 涉及任务生命周期、账本或定价的变更 MUST 附带可独立验证的测试。
- 编写与评审 MUST 为独立两轮；同一上下文 MUST NOT 自我批准。
- 任何违反本章程的复杂度 MUST 在计划的 Complexity Tracking 中显式记录并论证，否则门禁失败。
- 当前产品规格真源见 `specs/STATUS.md`（默认 `specs/003-open-collab-market/`）。

## Governance

- 本章程 supersede 所有其他实践与约定；冲突时以本章程为准。
- 修订程序：任何修订 MUST 以 PR 提交，说明动机与影响，并经维护者评审批准；涉及原则
  增删或重定义的修订 MUST 附带迁移说明。
- 版本策略遵循语义化版本：
  - MAJOR：不兼容的治理变更、原则移除或重定义。
  - MINOR：新增原则/章节或实质性扩展指导。
  - PATCH：澄清、措辞或拼写等非语义修订。
- 合规评审：所有 PR 与计划 MUST 验证对本章程的遵守；新参与者上手时 MUST 阅读本章程。
- 运行期开发指导参见各特性计划文档（见 CLAUDE.md 中 SPECKIT 标记区域）。

**Version**: 1.1.0 | **Ratified**: 2026-06-15 | **Last Amended**: 2026-07-12
