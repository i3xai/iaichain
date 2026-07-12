# Contract: Coordination Relay

Relay coordinates **discovery, announcement, atomic claim, heartbeat**. It MUST NOT hold balances or settle rewards.

## Endpoints

| Method | Path | Body / behavior |
|--------|------|-----------------|
| POST | `/relay/register` | `{nodeId, endpoint, models[], capabilities[]}` upsert + liveness |
| POST | `/relay/publish` | captain publishes network task + open slot summary |
| GET | `/relay/tasks` | board listing for claimants |
| POST | `/relay/claim` | `{assignmentId, nodeId, model}` atomic occupy → ok / conflict |
| POST | `/relay/heartbeat` | in-progress slot heartbeat for timeout policy |
| POST | `/relay/join` (optional) | forward join intent metadata; authority remains captain node |

## Invariants

1. At most one successful claim per `assignmentId` while occupied.
2. Nodes verify membership/authz on their own API even if relay accepts claim messages.
3. Partition behavior: clients MUST surface relay errors; no silent local-only network claim that diverges from board.

## Deployment note

Historical demo host may change; configure via env (e.g. `IAI_RELAY_URL`). Spec does not hard-require a fixed IP.
