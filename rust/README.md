# JudicialPredict — Rust Data Plane

Cargo workspace for the JudicialPredict Rust data plane.

## Workspace members

| Crate | Type | Tier |
|-------|------|------|
| `api-gateway` | binary | Imperative shell |
| `feature-store` | lib + binary | Imperative shell |
| `audit-recorder` | lib | Imperative shell |
| `feature-store-types` | lib | **FUNCTIONAL-CORE** |
| `decision-arith` | lib | **FUNCTIONAL-CORE** |
| `monte-carlo-sim` | binary | **FUNCTIONAL-CORE** |
| `cost-engine` | lib | **FUNCTIONAL-CORE** |
| `rate-limit` | lib | **FUNCTIONAL-CORE** |
| `ingest-fetcher` | binary | Imperative shell |
| `feature-deriver` | binary | Imperative shell |
| `event-broker` | binary | Imperative shell |
| `partner-gateway` | binary | Imperative shell |

See `adrs/adr-fp-001-functional-core-imperative-shell.md` for the boundary rules.

## Prerequisites

- Rust 1.95+ (install via [rustup](https://rustup.rs))
- `protoc` (protobuf compiler): `sudo apt-get install -y protobuf-compiler`
- `pkg-config` + `libssl-dev`: `sudo apt-get install -y pkg-config libssl-dev`
- `buf` (proto linter): installed at `/usr/local/bin/buf`
- Docker + Docker Compose (for Postgres dev stack)
- `sqlx-cli`: `cargo install sqlx-cli --features postgres,rustls --no-default-features`

After installing Rust, source the environment:
```bash
source "$HOME/.cargo/env"
```

## Build

```bash
cargo build --workspace
```

## Test

```bash
# Unit + property tests
cargo test --workspace

# With Postgres (required for feature-store + audit-recorder integration tests)
DATABASE_URL=postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev \
  cargo test --workspace

# e2e smoke tests (requires both api-gateway + feature-store-server to spawn)
DATABASE_URL=postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev \
  cargo test --test e2e_smoke -p api-gateway -- --include-ignored
```

## Dev stack (Postgres + Neo4j + Redis + MinIO)

```bash
# Start
docker compose -f ../docker-compose.dev.yml up -d

# Apply migrations
cd feature-store
DATABASE_URL=postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev \
  sqlx migrate run

# Stop
docker compose -f ../docker-compose.dev.yml down

# Reset (destroys volumes)
docker compose -f ../docker-compose.dev.yml down -v
```

See `docs/runbooks/dev-stack.md` for full connection strings and per-service CLI commands.

## Mutation testing

### Install cargo-mutants

```bash
cargo install --locked cargo-mutants
# Verify
cargo mutants --version
```

### Run manually on a single crate

```bash
# Example: rate-limit with 5-min timeout
cargo mutants -p rate-limit --no-shuffle --timeout 300 --output .mutants-rate-limit/

# Check results
cat .mutants-rate-limit/mutants.out/missed.txt    # surviving mutations (must be addressed)
cat .mutants-rate-limit/mutants.out/caught.txt    # mutations killed by tests
cat .mutants-rate-limit/mutants.out/unviable.txt  # mutations that don't compile
```

### Baseline report

`CARGO_MUTANTS_BASELINE.md` — per-crate survival counts and detail.
`.mutants-baseline.json` — machine-readable sidecar used by the weekly cron to detect regressions.

The `rate-limit` baseline is confirmed (0 surviving mutations as of 2026-05-09).
The three remaining functional-core crates (`decision-arith`, `monte-carlo-sim`, `feature-deriver`) will be measured on the first weekly cron run (Monday 2026-05-11 06:00 UTC).

### Weekly cron

Install the cron entry (runs every Monday at 06:00 UTC):

```bash
crontab -l | { cat; cat scripts/mutants-weekly.cron; } | crontab -
```

The script (`scripts/mutants-weekly.sh`):
- Runs cargo-mutants on all four functional-core crates with a 30-min timeout each
- Diffs against `.mutants-baseline.json`
- Posts a markdown summary to `$SLACK_WEBHOOK_URL` (if set), otherwise appends to `/var/log/jp-mutants-weekly.log`
- Updates `.mutants-baseline.json` only if all four crates complete successfully

### When a surviving mutation is found

1. Read the mutation description in `mutants.out/missed.txt` (shows file + line + operator).
2. Either add a new proptest that catches it (preferred), OR document in `CARGO_MUTANTS_BASELINE.md` why it's an acceptable survivor (e.g. equivalent mutation, dead code path).
3. Re-run cargo-mutants to confirm the new test catches it.
4. Update `.mutants-baseline.json` with the new `missed` count.

## Proto generation

```bash
# Generate Rust tonic stubs (done at build time via build.rs)
cargo build -p feature-store

# Regenerate manually if protos change
cd feature-store && cargo build  # tonic-build re-runs automatically
```

See `adrs/adr-002-grpc-contracts-source-of-truth.md` and `docs/runbooks/ci.md`.
