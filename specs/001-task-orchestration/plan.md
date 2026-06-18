# Implementation Plan: IAI 核心任务编排与结算闭环

**Branch**: `001-task-orchestration` | **Date**: 2026-06-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-task-orchestration/spec.md`

## Summary

构建 IAI 系统的核心闭环：任务发起方通过命令行提交自然语言任务，系统解析→分解→匹配→
执行→聚合→记账→结算，最终交付聚合结果。技术上采用 **Rust** 实现一个「Claude Code CLI 风格」
的终端优先程序：单一可执行 + 子命令 + 可选交互式 TUI，文本 I/O 协议（stdin/args → stdout，
错误 → stderr，支持 JSON 与人类可读双格式）。节点执行通过调用外部大模型 Provider API 完成，
账本采用哈希链式 append-only 存储实现防篡改与可独立核验。

## Technical Context

**Language/Version**: Rust 1.83+（edition 2021）

**Primary Dependencies**:
- `clap`（派生宏，子命令 CLI）、`tokio`（异步运行时）
- `serde` + `serde_json`（双格式 I/O 与契约序列化）
- `reqwest`（调用模型 Provider HTTP API）
- `rusqlite`（任务 / 账本本地持久化）
- `sha2`（账本哈希链）、`tracing` + `tracing-subscriber`（结构化日志/可观测）
- `ratatui` + `crossterm`（可选交互式 TUI，对标 Claude Code 终端体验）
- `thiserror` / `anyhow`（错误模型）

**Storage**: 本地 SQLite（`rusqlite`）。账本表为 append-only，每条记录含前序哈希形成哈希链
（tamper-evident）；任务/子任务状态、节点注册表同库分表。

**Testing**: `cargo test` + `cargo nextest`；契约测试置于 `tests/contract/`，集成测试置于
`tests/integration/`，单元测试随源码 `#[cfg(test)]`。

**Target Platform**: 跨平台 CLI（macOS / Linux / Windows），单一静态二进制为目标。

**Project Type**: CLI 工具（Cargo workspace，按章程四层分 crate）。

**Performance Goals**: 异步市场模型，无硬实时要求；对齐 SC-008——任务提交即返回标识，
节点供给充足时 95% 任务 ≤ 5 分钟到达 Settled；CLI 命令本地响应 < 200ms（不含模型调用）。

**Constraints**:
- 账本防篡改（哈希链 + append-only），可被参与方独立核验（章程 IV）。
- 无单点否决式中心组件；单节点失败不致任务整体失败（章程 III）。
- 仅通过节点能力契约交互，不耦合具体模型实现（章程 I）。
- 质量门禁未通过的结果不得进入结算（章程 VI / FR-008）。

**Scale/Scope**: V1 单编排器进程协调 N 个 IAI 节点；目标量级 ~数百并发任务、~数十至数百
注册节点；数据规模适中（本地 SQLite 足够，水平扩展为后续特性）。

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

依据 `.specify/memory/constitution.md` v1.0.0 的六条原则与两节附加约束逐条核验：

| 原则 | 门禁要求 | 本计划是否满足 |
|------|---------|--------------|
| I. 节点能力标准化 | 节点须有 node_id/capabilities/models 契约，仅经契约交互 | ✅ Node 注册契约 + 能力匹配仅读契约（见 contracts/node-contract） |
| II. 任务生命周期完整性 | 七态单向推进 + 可审计转移记录 | ✅ data-model 状态机 + 每次转移写审计记录（FR-002/016） |
| III. 去中心化与可调度 | 无 SPOF、节点可替换可容错、重试/重匹配 | ✅ 编排器将节点视作可替换资源 + 失败重匹配（FR-009） |
| IV. 经济可结算与可追溯 | 每次执行有账本记录、防篡改、可独立核验 | ✅ 哈希链 append-only 账本（FR-010/013） |
| V. 自由市场与公平定价 | 公开供需信号定价、对称可见、可复算 | ✅ Market Price 实体 + 公开定价规则（FR-012, SC-006） |
| VI. 可观测与质量验证 | 结算前质量门禁 + 关键路径结构化日志 | ✅ 自动质量门禁（FR-008）+ tracing 全链路 |
| 分层架构约束 | 四层、单向自上而下依赖 | ✅ Cargo workspace 按层分 crate（见 Project Structure） |
| 开发工作流与质量门禁 | Spec-Kit 流程 + 关键变更带可验证测试 + 编写/评审分离 | ✅ 契约测试 + 集成测试覆盖生命周期/账本/定价 |

**初次评估结论（Phase 0 前）**: PASS — 无违规，无需 Complexity Tracking。

**设计后复评（Phase 1 后）**: PASS — 设计未引入新的章程违规；分层 crate 边界保持单向依赖，
账本哈希链满足防篡改，质量门禁作为结算前置阶段建模。详见各设计工件。

## Project Structure

### Documentation (this feature)

```text
specs/001-task-orchestration/
├── spec.md              # Feature spec (/speckit-specify)
├── plan.md              # This file (/speckit-plan)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── cli-commands.md       # CLI 子命令契约
│   └── node-contract.md      # 节点注册与执行契约
└── checklists/
    └── requirements.md  # Spec 质量检查（已通过）
```

### Source Code (repository root)

按章程四层划分为 Cargo workspace，依赖方向自上而下单向：

```text
Cargo.toml                  # workspace 根
crates/
├── iai-cli/                # 应用层（Client Layer）：clap 子命令 + 可选 ratatui TUI
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/       # task submit / status / node register / ledger / market
│   │   └── tui/            # 交互式终端（Claude Code 风格）
│   └── tests/
├── iai-core/               # 核心调度层（IAI Core Layer）：编排器 / 解析 / 分解 / 匹配 / 聚合 / 质量门禁
│   ├── src/
│   │   ├── lifecycle.rs    # 七态状态机
│   │   ├── orchestrator.rs
│   │   ├── decompose.rs
│   │   ├── matcher.rs      # 综合评分匹配（FR-004）
│   │   ├── aggregate.rs
│   │   └── quality.rs      # 规则校验 + 裁判模型门禁（FR-008）
│   └── tests/
├── iai-node/               # 节点运行层（IAI Node Layer）：能力容器 + Provider 适配
│   ├── src/
│   │   ├── registry.rs     # 节点注册/发现
│   │   ├── node.rs         # 能力契约
│   │   └── providers/      # reqwest 调用 gpt/claude/qwen 等
│   └── tests/
└── iai-economic/           # 经济系统层（Economic Layer）：账本 / 贡献点 / 市场定价
    ├── src/
    │   ├── ledger.rs       # 哈希链 append-only（FR-010/013）
    │   ├── credit.rs       # 贡献点计量（FR-011）
    │   └── market.rs       # 公开供需定价（FR-012）
    └── tests/

tests/
├── contract/               # 契约测试（CLI 命令 + 节点契约）
└── integration/            # 端到端生命周期/容错/结算
```

**Structure Decision**: 采用单 workspace 多 crate，每个 crate 对应章程一层。依赖方向：
`iai-cli` → `iai-core` → {`iai-node`, `iai-economic`}；`iai-node` 与 `iai-economic` 不反向
依赖上层（满足分层架构约束的单向依赖）。CLI 为唯一对外接口，对标 Claude Code 的终端优先体验。

## Complexity Tracking

> 无需填写：Constitution Check 通过，无违规需要论证。
