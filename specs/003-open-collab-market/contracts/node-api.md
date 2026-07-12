# Contract: Node HTTP API

Base: `http://127.0.0.1:<port>` (loopback). Phase tags: **P1** / **P2** / **P3**.

## Roles & repo (P1)

| Method | Path | Notes |
|--------|------|-------|
| GET/POST/PUT/DELETE | `/api/roles`… | role_template CRUD; captain undeletable |
| POST | `/api/repo/check` | opensource `git ls-remote` / internal ssh rev-parse |

## Tasks (P1+)

| Method | Path | Notes |
|--------|------|-------|
| POST | `/api/tasks` | TaskCreate (repo, roles, reward, visibility) |
| GET | `/api/tasks` | list cards |
| GET | `/api/tasks/:id` | detail + roles + assignments |
| GET | `/api/tasks/:id/log` | op_log timeline |

TaskCreate body (unchanged shape from 002):

```jsonc
{
  "title": "…",
  "repo": { "kind": "opensource", "url": "https://github.com/…", "branch": "" },
  "roles": [{ "name": "后端", "prompt": "…", "recruitCount": 1, "modelFilter": "any" }],
  "reward": 0,
  "visibility": "network"
}
```

## Team join (P1) — specify / harden

| Method | Path | Notes |
|--------|------|-------|
| POST | `/api/team/join` | `{captainNodeId, role?, model?}` → pending on relay |
| GET | `/api/team/join-requests` | captain lists `{requests:[…]}` |
| POST | `/api/team/join-requests/decide` | `{applicantNodeId, approve, role?, model?}` |

Existing invite endpoints remain valid shortcuts.

## Claim / network (P1)

| Method | Path | Notes |
|--------|------|-------|
| GET | `/api/network/tasks` | open slots filter role/model/minReward |
| POST | `/api/assignments/:id/claim` | `{model}` → 200 or 409 |
| POST | `/api/match/auto` | optional enhancement |
| PUT | `/api/match/hosted` | optional enhancement |

Claim MUST require approved membership (FR-107) via relay join status (`approved` for publisher↔claimant), except the publisher claiming own slots.

## Models / agent (P2)

| Method | Path | Notes |
|--------|------|-------|
| GET | `/api/models/instances` | busy/idle, tokens, work seconds |
| POST | `/api/assignments/:id/start` | begin agent run |
| POST | `/api/assignments/:id/submit` | mark submitted (or agent auto-submit) |
| POST | `/api/assignments/:id/review` | captain `{verdict: accept\|reject\|kick, note?}` |

## Settlement (P3)

| Method | Path | Notes |
|--------|------|-------|
| POST | `/api/tasks/:id/settle` | only from Aggregated + gate pass |
| GET | `/api/wallet` / ledger routes | existing CLI/API surfaces |

Settlement refuses without evidence / failed gate.
