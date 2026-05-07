# CI Runbook — JudicialPredict

Explains each CI job, what it gates, and how to reproduce failures locally.

---

## Jobs

### `lint-rust` — `cargo fmt` + `cargo clippy`

**What it gates:** code style and basic correctness on every Rust crate in `rust/`.

**Runs on:** every PR and push to `main`.

**Reproduce locally:**
```bash
cd rust/
cargo fmt --all -- --check      # formatting (zero-tolerance)
cargo clippy --all-targets --all-features -- -D warnings   # lint (warnings are errors)
```

**Common failures:**
- `fmt` failure → run `cargo fmt --all` and commit.
- `clippy` warning → fix the flagged code; do not `#[allow(...)]` without a comment explaining why.
- `lld` linker not found locally → unset `RUSTFLAGS` or install `lld`: `sudo apt install lld`.

---

### `build-rust` — debug build + release check

**What it gates:** the workspace compiles in both debug and release profiles.

**Runs on:** every PR and push to `main`.

**Reproduce locally:**
```bash
cd rust/
cargo build --workspace            # debug
cargo check --workspace --release  # release profile type-check (faster than full build)
```

**Common failures:**
- Missing system deps → `sudo apt install pkg-config libssl-dev` (required by `openssl-sys` via `reqwest`).
- Feature flag conflicts → run with `--all-features` to surface hidden dependency issues.

---

### `test-rust` — tests + 70% line coverage gate

**What it gates:** all unit and integration tests pass; line coverage across the workspace stays ≥ 70% (spec §11.6.3). Functional-core crates (`feature-store-types`, `decision-arith`, `cost-engine`) are expected to exceed 80% because their property tests cover algebraic invariants.

**Runs on:** every PR and push to `main`.

**Reproduce locally:**
```bash
# Install cargo-llvm-cov if not present:
cargo install cargo-llvm-cov

cd rust/
cargo llvm-cov --workspace --all-targets --lcov --output-path lcov.info
cargo llvm-cov report --lcov --input-files lcov.info --fail-under-lines 70
```

**View HTML coverage report locally:**
```bash
cargo llvm-cov --workspace --all-targets --open
```

**Common failures:**
- Coverage drops below 70% → add tests to the crate(s) shown in the report.
- A test panics in a functional-core crate → check for I/O calls or mutable state (ADR-FP-001 Tier-1 rules).
- `llvm-tools-preview` component missing → `rustup component add llvm-tools-preview`.

---

### `proto-lint` — buf STANDARD ruleset

**What it gates:** all `.proto` files conform to the buf STANDARD ruleset (enum zero-value suffix, RPC request/response naming, package directory matching, doc comments, field naming).

**Runs on:** every PR and push to `main`.

**Reproduce locally:**
```bash
# Install buf if not present:
curl -sSL https://github.com/bufbuild/buf/releases/latest/download/buf-Linux-x86_64 -o /tmp/buf
chmod +x /tmp/buf && sudo mv /tmp/buf /usr/local/bin/buf

cd protos/
buf lint
```

**Common failures:**
- `PACKAGE_DIRECTORY_MATCH` → file must live in a directory that matches its package path (e.g. `judicialpredict/data_plane/feature_store/v1/`).
- `RPC_REQUEST_RESPONSE_UNIQUE` → each RPC must have its own dedicated request and response message; do not reuse `Feature` directly as an RPC return type.
- `RPC_RESPONSE_STANDARD_NAME` → response message must be named `<RpcName>Response` or `<ServiceName><RpcName>Response`.
- `ENUM_ZERO_VALUE_SUFFIX` → first enum value must end in `_UNSPECIFIED`.

---

### `proto-breaking` — buf breaking change detection

**What it gates:** no backward-incompatible changes to `.proto` contracts (field renames, field number changes, removed RPCs, type changes). Compares the PR branch against `github.base_ref` (the merge target).

**Runs on:** PRs only (skipped on push to `main` since there is no base to compare against).

**Reproduce locally:**
```bash
cd protos/
buf breaking --against ".git#branch=main,subdir=protos"
```

**Common failures:**
- Renaming a field → add a new field with the new name; deprecate (comment) the old one; remove in the next major version (`v2` package).
- Changing a field number → never do this; field numbers are permanent in proto3.
- Removing an RPC → mark it as deprecated in a doc comment and bump to `v2` before removal.

**Intentional breaking change process:**
1. Create a new package version (`v2`) in a new directory.
2. Update `INVENTORY.md` — set old package to `Deprecated`, add new package as `Active`.
3. Update ADR-002 with the migration timeline.
4. Open a migration PR that updates all consumers before removing the old package.

---

### `proto-format` — buf format check

**What it gates:** all `.proto` files are formatted per buf's canonical style (field alignment, spacing, import order).

**Runs on:** every PR and push to `main`.

**Reproduce locally:**
```bash
cd protos/
buf format --diff --exit-code   # shows diff without modifying files
buf format --write              # apply formatting in-place (then commit)
```

**Common failures:**
- Unformatted file → run `buf format --write` and commit the diff.

---

### `all-green` — aggregator (single required check)

**What it gates:** nothing directly — it exists so branch protection can require a single check (`all-green`) instead of listing every job individually. Configure GitHub branch protection to require `all-green` on `main`.

`proto-breaking` is allowed to be `skipped` (push-to-main path) and still passes the aggregator. All other jobs must be `success`.

---

## Local setup summary

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt clippy llvm-tools-preview
sudo apt install pkg-config libssl-dev lld
cargo install cargo-llvm-cov

# buf
curl -sSL https://github.com/bufbuild/buf/releases/latest/download/buf-Linux-x86_64 \
  -o /tmp/buf && chmod +x /tmp/buf && sudo mv /tmp/buf /usr/local/bin/buf
```

## Branch protection recommended settings

| Setting | Value |
|---------|-------|
| Required status check | `all-green` |
| Require branches to be up to date | ✓ |
| Dismiss stale reviews on new push | ✓ |
| Require linear history | ✓ (rebase-merge only) |
