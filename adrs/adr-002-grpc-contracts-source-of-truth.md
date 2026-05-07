# ADR-002: gRPC contracts as the single source of truth between planes

**Status:** Accepted
**Date:** 2026-05-07
**Author:** gigforge-engineer (dispatched but output off-topic on free model; PM-seeded as fallback)
**Reviewers:** gigforge-pm, gigforge-dev-backend, gigforge-dev-ai
**Spec references:** §7 (Technical Architecture — Cross-plane contract subsection), §11.6 (Engineering Methodology), §11.6.5 (SOLID — Interface Segregation + Dependency Inversion)
**Plane issue:** JP-3
**Supersedes / Related:** builds on ADR-001 (Polyglot architecture boundary).

## Context

ADR-001 commits us to a polyglot Rust + Python architecture with services on either side of a network boundary. Every cross-plane call therefore needs a wire format and a contract. The contract surface is wide:

- Rust API gateway → Python ML inference for case predictions, factor breakdowns, conformal intervals.
- Rust feature store → Python services reading features (with Tier/Sensitivity/PermittedUse enforcement preserved across the wire).
- Python decision orchestrator → Rust decision-arithmetic core for EV / CVaR / Nash bargaining hot loops.
- Python federated-learning coordinator → Rust gateway for secure-aggregation transport.
- Python ML inference → Rust Monte Carlo simulation engine for full-trajectory distributions.
- Django admin app → Rust feature store for all mutations (admin reads Postgres directly but writes via gRPC so compliance enforcement applies).

Without a single source of truth for every contract, three failures occur in real systems of this size:

1. **Schema drift.** One side changes a field, the other side compiles fine but deserializes garbage at runtime.
2. **Documentation rot.** Hand-written API docs diverge from actual endpoint behavior; reviewers can't verify a PR.
3. **Cross-language type mismatches.** A `Decimal` on one side becomes a `string` on the other, a `bool` becomes a `number`, an enum becomes an unconstrained string. Each mismatch is a runtime bug waiting for production traffic.

The contract layer must be **machine-checked**, **language-agnostic**, **review-friendly**, and **versioned with the same git history as the code that depends on it**.

## Decision

We adopt **Protocol Buffers (proto3) + gRPC** as the universal cross-plane contract, and the **`protos/` directory in the mono-repo as the canonical schema location**.

Specifics:

- **Schema location:** `protos/` at the mono-repo root. Subdirectories follow `protos/<plane>/<service>/<version>/*.proto` — for example `protos/data-plane/feature-store/v1/feature_store.proto`, `protos/ml-plane/inference/v1/inference.proto`. The `protos/` tree is the *only* place a contract is defined.
- **Rust codegen:** `prost` for message types + `tonic` for service stubs. Build script (`build.rs`) regenerates Rust source on every `cargo build` from the `protos/` files.
- **Python codegen:** `grpcio` + `grpcio-tools` invoked from `python/scripts/protoc-build.sh` — also runs in CI on every change. Generated stubs land in `python/<service>/_generated/` and are not hand-edited.
- **Schema linting:** `buf` (https://buf.build) runs `buf lint` on every PR with the recommended ruleset plus repo-specific rules (e.g., enums must end in `_UNSPECIFIED = 0` for forward-compat).
- **Breaking-change detection:** `buf breaking` runs on every PR against the `main` branch; incompatible changes block merge. Genuinely breaking changes require a new package version directory (`v2/`) — old `v1/` stubs continue to work until consumers migrate.
- **Versioning rules:** semantic-versioning of proto packages.
  - **Patch:** comments, naming, doc updates only.
  - **Minor:** additive only — new optional fields, new RPCs, new enum values, new services. Old clients keep working.
  - **Major:** removal of fields / RPCs / enum values, type changes, semantic changes. Requires `v(N+1)/` directory; both versions cohabit until migration completes.
- **Cross-plane integration tests:** every `.proto` change triggers a CI integration test stage that spins up matched Rust + Python service images and runs a roundtrip protocol test (encode in Rust → decode in Python and vice versa, plus a service-level end-to-end happy-path).
- **Schema review:** every `.proto` change requires an additional reviewer flagged by `CODEOWNERS` for the proto package — typically the lead engineer plus one engineer from the consuming plane. Schema review precedes implementation review.
- **Wire format:** binary protobuf for all internal traffic. JSON-mapped protobuf only at user-facing API edges (and only via the GraphQL gateway, never raw).
- **Streaming:** server-streaming and bidirectional streaming RPCs allowed where the data shape demands it (Monte Carlo trajectory streaming, federated-learning gradient updates, real-time event broker fan-out). Unary RPCs preferred when the data fits.
- **Auth propagation:** every gRPC call carries a request-scoped `Authorization` metadata header (per-tenant JWT → operator-scoped service account between planes). The Rust feature-store enforces tier-based authorization on inbound metadata before any data lookup.

## Consequences

### Positive

- **Compile-time + serialize-time guarantees on both planes.** A field rename on the Rust side fails the Python codegen on the same PR, surfacing the drift before merge. Type mismatches cannot reach runtime.
- **Single review surface.** Every cross-plane API change is one PR touching `protos/` + the implementations on both sides. Reviewers see the full picture.
- **Documentation is the schema.** Comments in `.proto` files become docstrings in generated Rust + Python code; `buf generate --template buf.gen.docs.yaml` produces a browsable doc site automatically.
- **Forward / backward compatibility built in.** Adding optional fields never breaks old clients; the proto3 wire format ignores unknown fields and zero-fills missing ones. Rolling deploys work.
- **Performance.** Binary protobuf is 3-10× smaller than JSON and 5-20× faster to encode/decode. The gRPC HTTP/2 multiplexed stream avoids head-of-line blocking that plagues REST + JSON over HTTP/1.1.
- **Streaming is first-class.** Server-stream + bidi-stream handle the Monte Carlo trajectory + FL gradient-update use cases natively; no SSE workarounds.

### Negative

- **Schema-design overhead up front.** Both teams must align on field shapes before either implements. Mitigated by Three Amigos sessions on every schema change (per §11.6.4 BDD methodology).
- **Tooling install.** Engineers need `protoc`, `prost-build` (Rust), `grpcio-tools` (Python), `buf` CLI, and the IDE plugins for syntax highlighting. Mitigated by devcontainer / Nix flake bundled in the repo (Sprint 2 task).
- **Generated code in the repo.** We generate-and-commit Python stubs under `python/<service>/_generated/` so Python services can run without a build step. Trade-off: PR diffs include generated files; mitigated by a `.gitattributes` mark to collapse generated diffs in code review.
- **Opaque debugging.** Binary protobuf is not human-readable on the wire. Mitigated by `grpcurl` for ad-hoc inspection and by per-service request/response tracing through OpenTelemetry.

### Neutral / mitigations

- **Reversibility:** If gRPC proves unsuitable later (unlikely), the `protos/` schemas convert cleanly to OpenAPI-described REST via `gnostic-grpc` or to MessagePack-RPC via straightforward codegen swaps. The schema *language* is the true contract; the *transport* can be swapped.
- **Risk: contract overgrowth.** Without discipline, every internal call gets its own RPC and the proto tree balloons. Mitigated by code-review enforcement of the principle "the gRPC API is for cross-plane traffic only — within a service, use direct function calls or in-process channels."
- **Risk: enum value churn.** Enum changes are easy to get wrong (e.g., reusing a numeric value with a new name silently corrupts old serialized data in storage). Mitigated by `buf breaking` rules that forbid enum-value reuse.

## Alternatives considered

### Alternative A — REST + JSON (FastAPI on the Python side, axum-OpenAPI on the Rust side)
**Rejected.** Mature ecosystem and human-readable wire format are real strengths, but every drawback hits us hard. There is no machine-checked schema source-of-truth (OpenAPI helps but is not enforced at compile time on either plane); JSON encoding is 5-20× slower than protobuf for the volume of decision-layer + Monte Carlo traffic; unary-only without HTTP/2 streaming workarounds; type system is anemic (no enums, no oneof, no proper bytes). Acceptable for the public partner API and the customer-facing GraphQL, where humans interact directly. Wrong for our hot internal traffic.

### Alternative B — OpenAPI/Swagger-driven REST with codegen (openapi-generator)
**Rejected.** Mitigates some of REST+JSON's drawbacks (machine-checked schema if you write it in OpenAPI first) but still loses on wire performance, lacks proper streaming, and the openapi-generator output for both Rust and Python is significantly less ergonomic than `tonic` and `grpcio` outputs. The OpenAPI tooling ecosystem is the right answer at the partner-API edge (§11), not for cross-plane.

### Alternative C — MessagePack-RPC / msgpack-rpc-rs ↔ msgpack-rpc-python
**Rejected.** Binary wire format like protobuf, but the schema-definition language is the wild west — typically you write Rust types and Python types separately and hope they agree. No single source of truth, no `buf breaking` equivalent, no streaming, much smaller community. We would be hand-rolling the safety machinery that gRPC + buf give us for free.

### Alternative D — Custom binary protocol (capnproto, flatbuffers, sbe)
**Rejected.** Cap'n Proto and FlatBuffers offer zero-copy reads and faster parsing than protobuf, but the Rust + Python tooling for either is markedly less mature than for protobuf, and the marginal performance is not the bottleneck for our traffic shape (Monte Carlo throughput is bounded by the Rust trajectory-generation cost, not the wire). The maintenance cost of choosing a less-common contract format outweighs the benefit.

### Alternative E — Raw TCP / WebSockets with hand-defined frame formats
**Rejected.** Reinvents a wheel that is everyone else's wheel for no benefit. Documented for completeness only.

## Compliance and verification

- **CI gate (per PR):**
  - `buf lint` — schema linting against repo + recommended rules.
  - `buf breaking --against '.git#branch=main'` — block incompatible changes.
  - `cargo build` / Python codegen — block schemas that don't compile in either language.
  - **Cross-plane integration test** — runs matched Rust + Python service images, makes a real gRPC roundtrip per changed RPC, asserts response-shape parity.
- **CODEOWNERS** for `protos/` requires the lead engineer + one engineer from the consuming plane on every schema PR.
- **Three Amigos session** mandatory for every story touching `protos/` (per §11.6.4 BDD methodology) — PO + Dev + QA align on contract before implementation begins.
- **Generated-code provenance:** `python/<service>/_generated/` files carry a generator stamp + source-proto SHA at the top; mismatched stamps fail CI.
- **Contract test suite:** `tests/contract/` holds Gherkin scenarios for every public RPC; specifications double as living documentation per §11.6.4.
- **Schema-version inventory:** `protos/INVENTORY.md` lists every package + version + status (Active / Deprecated / Sunset). Updated on every release.

## References

- `judicialpredict-v2-spec.md` §7 (Technical Architecture)
- `judicialpredict-v2-spec.md` §11.6 (Engineering Methodology), §11.6.4 (BDD), §11.6.5 (SOLID — ISP + DIP)
- ADR-001 (Polyglot Rust + Python + Django + Next.js architecture boundary)
- buf documentation: https://buf.build/docs
- `prost`: https://github.com/tokio-rs/prost
- `tonic`: https://github.com/hyperium/tonic
- `grpcio` (Python): https://grpc.io/docs/languages/python/

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*

*Note on authorship: this ADR was authored by the PM after the gigforge-engineer agent dispatch returned off-topic content (the local Gemma 4 e4b-class model on the dev Hetzner is too weak for multi-step file-I/O instruction following). The agent will be re-engaged for ADR-003 and ADR-004 once a stronger model is available, or those will be PM-authored as well. The seed-then-iterate pattern continues.*
