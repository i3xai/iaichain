# Spec Status · IAI Chain

**Current source of truth**: [`003-open-collab-market/`](./003-open-collab-market/)  
**Constitution**: [`.specify/memory/constitution.md`](../.specify/memory/constitution.md) v1.1.0

## Document map

| Path | Role | Status |
|------|------|--------|
| [`003-open-collab-market/`](./003-open-collab-market/) | Product spec (open collab market + coding agent) | **Current** |
| [`001-task-orchestration/`](./001-task-orchestration/) | V1 baseline (CLI lifecycle, ledger, market) | Archived |
| [`002-collaborative-task-market/`](./002-collaborative-task-market/) | V2 design notes (roles, relay, worktree) | Archived |
| [`../IAI_system_design_v1.md`](../IAI_system_design_v1.md) | Early vision summary | Superseded by 003 |
| [`../DEVELOPMENT-PLAN.md`](../DEVELOPMENT-PLAN.md) | Historical stages 0–7 | Historical; delivery follows 003 phases |

## Delivery phases (closed loops)

| Phase | Loop | Spec stories | Implementation (rough) |
|-------|------|--------------|------------------------|
| **1** | Publish → join team → claim slot | US1 | `partial` → join/approve/claim gate landed; harden multi-instance demo |
| **2** | Agent work → submit → captain review | US2 | `partial` — LLM + worktree exist; tool-loop agent + real review are gaps |
| **3** | Quality → settle → verifiable market | US3 | `partial` — ledger/market exist; task-bound pricing + anti-fraud baseline are gaps |

## Maturity tags

Used on each FR in `003/spec.md`:

- `implemented` — tested / demoable
- `partial` — main path exists, missing open-network / agent / anti-fraud pieces
- `specified` — spec only; implementation may lag (by design)

## Active focus

Phase 1 join/claim gate is in code. Next engineering focus: **Phase 2 coding agent tool loop** (unless Phase 1 multi-instance demo still needs polish).
