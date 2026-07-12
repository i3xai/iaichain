# Feature Specification: Open Collaborative Task Market

**Feature Branch**: `003-open-collab-market`  
**Created**: 2026-07-12  
**Status**: Current (source of truth)  
**Replaces**: `001-task-orchestration` (baseline), `002-collaborative-task-market` (design notes)

**Input**: Product boundary workshop 2026-07-12 — small-team delivery; full open-market vision in spec; coding agent tool loop; serious contribution-credit market; implementation may lag (`specified`).

## Clarifications

### Session 2026-07-12

- Q: Near-term delivery target? → A: Small team, real git repos, full publish→claim→work→review→settle path.
- Q: Execution depth? → A: Real coding agent (tool loop: read/write/run/git) until role goal or limits.
- Q: Network trust? → A: Open market style — nodes may register and claim; spec written for open, not private-only.
- Q: Economics? → A: Serious market — public pricing, recalculable, anti-fraud baseline; no fiat/on-chain.
- Q: Spec vs 6-month delivery? → A: Spec holds full vision; implementation gaps allowed and tagged.
- Q: Delivery slicing? → A: Three closed loops — (1) publish/join/claim (2) collaboration (3) settlement.

## Maturity legend

Each FR is tagged: `implemented` | `partial` | `specified`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Publish, join, claim (Priority: P1) · Phase 1

Captain publishes a role-based task on a reachable repo. Other nodes apply to join the team, get approved, then claim open assignment slots. No agent execution required for this loop to be valuable.

**Why this priority**: Without a filled roster and claimed slots, collaboration and settlement have no subjects.

**Independent Test**: Two nodes + relay (or local multi-instance): create network task → join request approved → claim succeeds; double-claim returns conflict.

**Acceptance Scenarios**:

1. **Given** a captain node, **When** it creates a task with ≥1 non-captain role and recruit_count ≥1, **Then** matching `open` assignments exist and task is `Recruiting`.
2. **Given** a registered external node, **When** it submits a join request, **Then** status is `pending` until captain approves or rejects.
3. **Given** an approved member and a matching `network` open slot, **When** it claims with a valid model, **Then** slot becomes `claimed` and cannot be claimed by others.
4. **Given** two nodes claim the same slot concurrently, **When** relay atomic claim runs, **Then** exactly one succeeds; the other gets a clear conflict.
5. **Given** a node that is not an approved member, **When** it attempts claim, **Then** the system refuses with a reason.

---

### User Story 2 — Collaborate with coding agent + review (Priority: P2) · Phase 2

Claimed slots run a coding agent inside an isolated worktree. Outputs are submitted; captain reviews (accept / reject / kick). Heartbeat timeout returns slots to the market.

**Why this priority**: This is the product’s core “real work” value for small teams.

**Independent Test**: One claimed slot with a real provider completes an AgentRun, commits to worktree, passes captain accept to `done`; a reject path re-enters work; a kick reopens the slot.

**Acceptance Scenarios**:

1. **Given** a `claimed` slot and a usable model, **When** execution starts, **Then** status is `working`, an `AgentRun` is recorded, and tools stay within the tool contract.
2. **Given** the agent finishes the role goal, **When** it submits, **Then** a commit exists in the assignment worktree and status is `submitted`.
3. **Given** captain review fails the bar, **When** reject, **Then** slot returns to rework (`working` or equivalent) and attempts increment; at limit, kick → `open`.
4. **Given** missing heartbeats (e.g. 3 failures), **When** timeout policy fires, **Then** kick, worktree released, slot `open` for re-claim.
5. **Given** all required slots are `done`, **When** captain confirms aggregation, **Then** task is `Aggregated` and **not** yet `Settled`.

---

### User Story 3 — Settlement and verifiable market (Priority: P3) · Phase 3

After aggregation and quality gates, the system settles contribution credits on a tamper-evident ledger. Prices follow public supply/demand rules and are recalculable. An anti-fraud baseline applies to open participation.

**Why this priority**: Trust and incentives for an open network; not required to demo Phase 1/2 loops.

**Independent Test**: Settled task has ledger entries matching reward_alloc; third party recalculates price from public snapshot; verify chain; gate failure blocks Settled.

**Acceptance Scenarios**:

1. **Given** `Aggregated` with complete execution evidence, **When** settle, **Then** task is `Settled`, each allocation has ledger entries, chain verifies.
2. **Given** `reward_total > 0`, **When** settle, **Then** lock releases and done role nodes split per public rule (captain does not take dev reward).
3. **Given** `reward_total = 0`, **When** settle, **Then** each done role node receives the configured floor (default +1 credit).
4. **Given** public pricing rule + demand/supply snapshot, **When** recalculated offline, **Then** result matches system record.
5. **Given** failed quality gate or missing evidence, **When** settle attempted, **Then** refused.
6. **Given** anti-fraud baseline (identity, deposit, or reputation — at least one), **When** a node fails the bar, **Then** claim and/or reward is refused or reduced per written rule.

---

### Edge Cases

- Unreachable repo at create time → reject create; do not enter Recruiting.
- No matching model for a role → slot stays open; captain sees waiting reason.
- Agent exceeds step/token limit → fail or submit partial per policy; must not silent-success.
- Relay unavailable → local private tasks may continue; network publish/claim MUST surface clear errors (no silent split-brain claim).
- Ledger verify failure → refuse new Settled until repaired or investigated.

## Requirements *(mandatory)*

### Phase 1 — Publish / join / claim

- **FR-101** `partial`: Captain MUST create tasks with repo config, roles, recruit counts, visibility (`private`|`network`).
- **FR-102** `partial`: System MUST create `open` assignments from recruit_count and enter `Recruiting`.
- **FR-103** `specified`: Nodes MUST register identity (`node_id`, capabilities, models) for open discovery.
- **FR-104** `partial`: Nodes MUST request to join a team; captain MUST approve/reject (`JoinRequest`).
- **FR-105** `partial`: Approved members MUST claim open slots with model filter checks; atomic claim prevents double-claim.
- **FR-106** `partial`: Relay MUST support register, publish, list tasks, atomic claim (discovery/negotiation only; ledger stays local).
- **FR-107** `partial`: Unapproved nodes MUST NOT claim.

### Phase 2 — Agent collaboration / review

- **FR-201** `specified`: Execution MUST use a coding agent tool loop (read/write/list/run/git) until goal, limit, or failure.
- **FR-202** `partial`: Each assignment MUST use an isolated worktree; commits attributable to role/slot.
- **FR-203** `partial`: Assignment state MUST support `claimed→working→submitted→done` with reject/kick edges.
- **FR-204** `partial`: Captain MUST review submitted work (accept/reject/kick); outcomes audited in `op_log`.
- **FR-205** `partial`: Working slots MUST heartbeat; timeout kicks and reopens slot.
- **FR-206** `partial`: Tool use MUST obey declared tool contract; out-of-tree paths rejected.
- **FR-207** `specified`: Container/strong sandbox MAY lag; worktree boundary is the minimum isolation for open network.

### Phase 3 — Settlement / market / anti-fraud

- **FR-301** `partial`: Settled MUST require ledger evidence bound to execution records.
- **FR-302** `partial`: Reward rules MUST be public: split among done role nodes if reward_total>0; else floor; captain excluded from dev reward.
- **FR-303** `partial`: Ledger MUST be append-only hash chain and independently verifiable.
- **FR-304** `specified`: Pricing MUST use public supply/demand signals and be recalculable by participants.
- **FR-305** `specified`: At least one anti-fraud baseline MUST apply (identity binding, claim deposit, or reputation threshold).
- **FR-306** `partial`: Failed quality gate MUST block Settled.
- **FR-307** `specified`: Quality gate SHOULD progress from deterministic rules to model-as-judge / captain-agent review without changing the “no gate → no settle” contract.

### Cross-cutting

- **FR-001** `implemented`: Interaction via node capability contracts only (no hard-coded model coupling in matcher).
- **FR-002** `partial`: State transitions MUST be recorded for audit (inputs, node, result fingerprint where applicable).
- **FR-003** `partial`: Participants MUST query task status, own contributions, and ledger history.

## Key Entities

- **Node**, **Team**, **JoinRequest**
- **Task**, **TaskRole**, **Assignment**
- **AgentRun**, **Worktree**, **ToolContract**, **Review**
- **LedgerEntry**, **RewardAlloc**, **MarketPrice**, **AntiFraudBaseline**
- **OpLog**, **RelayAnnouncement**

## Success Criteria *(mandatory)*

- **SC-101**: Two-node demo completes Phase 1 without agent (create → approve join → claim; double-claim safe).
- **SC-201**: ≥1 role slot produces a real git commit via agent path and reaches `done` after captain accept.
- **SC-202**: Kick/timeout returns a slot to `open` and another node can claim it.
- **SC-301**: 100% of Settled tasks have verifiable ledger entries matching allocations.
- **SC-302**: Offline recalculation of a published price matches system record for sampled tasks.
- **SC-303**: 100% of gate failures never reach Settled.
- **SC-001**: Spec documents full open-market vision; `STATUS.md` shows maturity gaps without blocking Phase 1 demos.

## Assumptions

- Contribution credits are internal units; no fiat/on-chain exchange in this feature.
- Coordination relay is acceptable for discovery/claim; pure P2P is future work (constitution v1.1).
- Small-team ops may start with few nodes; open registration remains in scope for the spec.
- Default branch naming `task/<task_id>`; worktree `<repo>/.worktrees/<role>-<slot>`.
- V1 subtask path may remain for CLI demos; V2 assignment path is the product path.

## Out of Scope

- Fiat / on-chain token exchange
- Pure P2P without relay
- General non-git AI capability marketplace (reasoning/writing mall) as primary product
- Replacing a full IDE; this is orchestration + agent execution
