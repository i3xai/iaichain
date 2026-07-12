# Quickstart: Open Collaborative Task Market

Aligned with Phase 1 closed loop (publish → join → claim). Phases 2–3 build on the same binary.

## Prerequisites

```sh
cargo build --release
# optional: configure a provider for Phase 2
./target/release/iai model add ollama --model llama3
```

Data dir: `$IAI_HOME` (default `~/.iai`).

## Single node (captain)

```sh
./target/release/iai serve --port 8787
# open http://127.0.0.1:8787/console
# create task: repo + roles + reward + visibility=network
```

## Two-node Phase 1 demo (recommended)

Use two homes and one relay (see `contracts/relay.md`):

```sh
# terminal A — captain
IAI_HOME=~/.iai-captain ./target/release/iai serve --port 8787

# terminal B — member
IAI_HOME=~/.iai-member ./target/release/iai serve --port 8788
```

1. Captain: create team / publish network task with open slots.  
2. Member: register with relay, **apply to join**, wait for captain approve.  
3. Member: `GET` network tasks → `claim` an open assignment.  
4. Verify: second claim on same slot → conflict; member slot status `claimed`.

Phase 2: start agent on claimed slot, confirm worktree commit + captain accept.  
Phase 3: settle and `iai ledger verify`.

## Docs

- Spec: [spec.md](./spec.md)  
- Status: [../STATUS.md](../STATUS.md)  
- Usage (ops): [../../USAGE.md](../../USAGE.md)
