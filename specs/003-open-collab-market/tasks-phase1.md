# Phase 1 Implementation Plan — Publish / Join / Claim

> **For agentic workers:** Implement task-by-task. Checkboxes track progress.

**Goal:** Close Phase 1 loop: captain publishes tasks; member applies → captain approves; member claims open slot; double-claim conflicts; unapproved claim refused.

**Architecture:** `join_request` mirrored locally + authoritative pending/approved state on coordination relay (same pattern as task board). Claim checks relay approval for `(publisher, applicant)` before atomic occupy. Captain approve also upserts local `team_member` (invite path).

**Tech Stack:** Rust (`iai-cli` storage/api/relay), SQLite migration v11, console HTML/JS via `web/shared/api.js`.

---

## File map

| File | Responsibility |
|------|----------------|
| `crates/iai-cli/src/storage.rs` | v11 `join_request`; list/create/decide; `is_member` |
| `crates/iai-cli/src/relay.rs` | `/relay/join`, `/relay/joins`, `/relay/join/decide`; client helpers |
| `crates/iai-cli/src/api/mod.rs` | `/api/team/join*`; claim membership gate |
| `web/shared/api.js` | API client wrappers |
| `web/console/console.html` + `app.js` | Apply + pending approve UI |
| `crates/iai-cli/tests/cli.rs` or unit tests | Join + membership behavior |

## Tasks

- [x] **T1** Migration v11 + storage CRUD for `join_request`
- [x] **T2** Relay join board + decide + client helpers
- [x] **T3** Node API: join / list / decide; invite syncs relay approved
- [x] **T4** `network_claim` (+ auto_match): refuse unless approved member or self-publisher
- [x] **T5** Console: apply form + captain pending list
- [x] **T6** Tests + `cargo test --workspace`

## Done when

SC-101: two homes + relay — apply → approve → claim ok; second claim 409; unapproved 403.
