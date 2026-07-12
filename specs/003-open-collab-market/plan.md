# Implementation Plan: Open Collaborative Task Market

**Branch**: `003-open-collab-market` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-open-collab-market/spec.md`

## Summary

Evolve IAI from V1 orchestration + V2 role/relay demos into a **specified open collaborative coding market**: captain publishes tasks, nodes join and claim, coding agents execute in worktrees, captain reviews, then verifiable contribution-credit settlement. Delivery is three closed loops (Phase 1→2→3). Spec holds full vision; FR maturity tags allow implementation lag.

## Technical Context

**Language/Version**: Rust 1.83+ (edition 2021)

**Primary Dependencies**: clap, tokio, axum, rust-embed, serde/serde_json, reqwest, rusqlite, sha2, tracing, thiserror/anyhow; frontend: native HTML/CSS/JS in `web/`

**Storage**: Local SQLite per node (`$IAI_HOME`); relay is discovery/claim coordination only

**Testing**: `cargo test --workspace`; contract + integration tests per phase acceptance

**Target Platform**: Cross-platform CLI binary `iai` (macOS / Linux / Windows)

**Project Type**: CLI + embedded local HTTP console (Cargo workspace, four crates)

**Performance Goals**: Async market; Phase 1 claim latency interactive; no hard realtime SLA beyond prior SC-008 guidance where still applicable

**Constraints**: Constitution v1.1; hash-chain ledger; capability contracts; quality gate before Settled; agent tool contract; open-network anti-fraud baseline (spec)

**Scale/Scope**: Small teams first; open registration in scope; hundreds of concurrent tasks aspirational

## Constitution Check

| Principle | Plan alignment |
|-----------|----------------|
| I Capability contracts | Node register + model filters on claim |
| II Lifecycle integrity | Extended states + audit/op_log; map to baseline seven states |
| III Decentralization | Relay for discovery/claim only; no ledger SPOF; Complexity: relay is coordination aid, not settlement authority |
| IV Economic settlement | Hash-chain ledger; Settled requires evidence |
| V Fair pricing | Public recalculable rules (Phase 3) |
| VI Quality / observability | Gate before Settled; op_log + tracing |
| Layering | `iai-cli → iai-core → {iai-node, iai-economic}` unchanged |
| Workflow | Spec-Kit + tests on lifecycle/ledger/pricing changes |

**Phase 0 gate**: PASS with Complexity note — coordination relay explicitly allowed under constitution III (v1.1).

## Project Structure

### Documentation (this feature)

```text
specs/003-open-collab-market/
├── spec.md
├── plan.md                 # this file
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── node-api.md
│   ├── relay.md
│   └── agent.md
└── checklists/requirements.md
```

### Source (unchanged layout)

```text
crates/iai-cli|iai-core|iai-node|iai-economic
web/landing|console|shared
tests/ as needed
```

**Structure Decision**: Keep four-layer workspace; Phase 2 agent runtime lives primarily in `iai-cli` (orchestration) + `iai-core` (contracts/quality), providers in node/cli adapters.

## Phased delivery

| Phase | Closed loop | Exit criteria |
|-------|-------------|----------------|
| 1 | Publish → join → claim | SC-101 |
| 2 | Agent → submit → review | SC-201, SC-202 |
| 3 | Settle → market → anti-fraud | SC-301–303 |

Prefer finishing Phase 1 gaps (JoinRequest, claim authz) before deep agent work, unless blocking demos.

## Complexity Tracking

| Violation / tension | Why needed | Simpler alternative rejected because |
|---------------------|------------|--------------------------------------|
| Coordination relay | Open claim needs atomic cross-node lock | Pure P2P too heavy for near-term open market |
| Spec ahead of impl | Product chose full vision in docs | Cutting FR would reintroduce doc drift |
