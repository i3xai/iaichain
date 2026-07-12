# Data Model: Open Collaborative Task Market

Evolves archived `002` models. Phase column marks when the entity is required for that closed loop.

## 1. Tables

### 1.1 node / team (Phase 1)

Existing node identity: `node_id`, capabilities, models, online.

**team_member**: captain + members (existing invite path remains; join-request is additive).

### 1.2 join_request (Phase 1) — new / harden

| Column | Type | Notes |
|--------|------|-------|
| id | INTEGER PK | |
| team_captain | TEXT | Captain node_id |
| applicant_node | TEXT | |
| status | TEXT | `pending` \| `approved` \| `rejected` |
| created_at / decided_at | TEXT | |

### 1.3 task (Phase 1+)

Extensions from V2: `repo_kind`, `repo_url`, `server_host`, `server_path`, `branch`, `reward_total`, `reward_locked`, `captain_node`, `visibility`, `archived_at`.

**Task state (product path)**:

```text
Created → Parsed → Decomposed → Recruiting → Executing → Reviewing → Aggregated → Settled
```

Mapping to constitution baseline seven states:

| Product state | Baseline anchor |
|---------------|-----------------|
| Created/Parsed/Decomposed | same |
| Recruiting | Matched (slots open / claiming) |
| Executing | Executed |
| Reviewing | Aggregated (pre-accept) |
| Aggregated | Aggregated (accepted) |
| Settled | Settled |

### 1.4 role_template / task_role (Phase 1)

Unchanged from 002: captain template non-deletable; `recruit_count`, `model_filter`.

### 1.5 assignment (Phase 1–2)

```text
open ──claim──► claimed ──start──► working ──submit──► submitted ──accept──► done
  ▲                                  │                      │
  └──────── kick ────────────────────┴──────reject──────────┘
```

Phase 1 exit: `claimed` is enough. Phase 2: full machine.

### 1.6 agent_run (Phase 2)

| Column | Notes |
|--------|-------|
| id, assignment_id, node_id | |
| status | `running` \| `succeeded` \| `failed` \| `limited` |
| steps_json | Tool trace summary |
| tokens, duration_ms | |
| exit_reason | goal \| step_limit \| token_limit \| error |

### 1.7 model_instance / op_log (Phase 2)

As in 002: single (node, model) busy constraint; op_log actions include join/claim/agent/review/kick/settle.

### 1.8 reward_alloc / ledger (Phase 3)

Ledger kinds: Lock / Unlock / Settle / Reward (existing).  
Reward rules (confirmed from 002):

- `reward_total > 0`: split among **done** role nodes; captain excluded; no floor stacked.
- `reward_total = 0`: floor **+1** credit per done role node.

### 1.9 anti_fraud (Phase 3) — specified

Minimal viable: one of

- **identity**: verified node binding before claim rewards
- **deposit**: lock small credit to claim; slash/refund rules public
- **reputation**: min success rate / settled count to claim high-reward tasks

Exact formula lives in plan/research when implementing; behavior MUST be public and recalculable.

### 1.10 relay side (Phase 1)

Announcements mirror `visibility=network` tasks + open assignments; claim is atomic occupancy. Ledger never through relay.

## 2. Economic rules (Phase 3)

See FR-302–305 in spec. Currency: single **contribution credit** (贡献币).

## 3. Naming

- Branch: `task/<task_id>` when empty at create
- Worktree: `<repo>/.worktrees/<role>-<slot>`
