# Research: Open Collaborative Task Market

**Date**: 2026-07-12  
**Purpose**: Phase 0 decisions for 003 (coding agent, open claim, serious market, phased delivery).

## 1. Coordination: relay vs P2P

**Decision**: Keep lightweight coordination relay for register / publish / list / atomic claim / heartbeat.

**Rationale**: Open double-claim needs a single occupancy authority; full P2P + NAT is out of near-term scope. Constitution v1.1 records this as allowed coordination, not settlement SPOF (ledger remains local/autonomous).

**Alternatives rejected**: Pure mDNS/DHT for claim locks (complexity, conflict resolution cost).

## 2. Coding agent runtime

**Decision**: Spec requires tool-loop agent (read/write/list/run/git) bounded to assignment worktree; step/token limits mandatory.

**Near-term implementation path**: Extend current `llm.rs` + `worktree.rs` into an agent loop in `iai-cli` (or `iai-core` trait + cli runner). Mock remains for offline tests.

**Alternatives**: One-shot LLM commit (current) — insufficient for User Story 2; external agent binary — deferred to avoid splitting observability.

**Sandbox**: Worktree path jail first; OS/container isolation tagged `specified` (FR-207).

## 3. Join model: invite vs apply

**Decision**: Formalize **JoinRequest** (apply → captain approve) as Phase 1 path for open market; keep invite as convenience.

**Rationale**: Open registration without approval collapses into spam claims; approval is the lightest gate before Phase 3 anti-fraud.

## 4. Quality gate evolution

**Decision**: Keep behavioral contract “no gate → no Settled”. Implementation may stay deterministic until captain-agent / model-as-judge lands (FR-307).

**Rationale**: Avoid blocking Phase 1/2 demos on judge model availability; still forbids silent settle.

## 5. Pricing & anti-fraud

**Decision**: Document public recalculable pricing and at least one anti-fraud baseline in Phase 3; reuse existing order-book market for credit exchange among nodes where applicable.

**Open formula**: Final supply/demand function chosen at Phase 3 implement time; MUST be versioned and published (constitution V).

## 6. Spec strategy

**Decision**: Single current spec (003) with maturity tags; archive 001/002; do not maintain a separate “6-month cut” spec.

**Rationale**: User chose full vision in docs with implementation gaps visible via `STATUS.md`.
