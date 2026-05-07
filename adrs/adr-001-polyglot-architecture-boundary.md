# ADR-001: Polyglot Rust + Python + Django + Next.js architecture boundary

**Status:** Accepted
**Date:** 2026-05-07
**Author:** gigforge-engineer (seeded by PM during agent-stall recovery; engineer to review and amend)
**Reviewers:** gigforge-pm, gigforge-dev-backend, gigforge-dev-ai
**Spec references:** §7 (Technical Architecture), §11.6.7 (Pragmatic Functional Programming)
**Plane issue:** JP-3

## Context

JudicialPredict's architecture must serve four very different concerns:

1. **CPU-bound or high-concurrency data-plane work** — API gateway, feature store with compile-time tier enforcement, ingestion pipelines, real-time event broker, Monte Carlo trial simulation, distributional cost engine, decision-arithmetic core. These are latency-critical, embarrassingly parallel, and benefit from compile-time type safety on the compliance hot path.
2. **ML / NLP / graph-ML / logic / federated-learning research and serving** — XGBoost / LightGBM / CatBoost / PyMC / NumPyro / spaCy / Hugging Face / DoWhy / EconML / owlready2 / Z3 / Flower / PySyft. These ecosystems are Python-first; Rust equivalents either don't exist (CEVAE, Flower) or are immature (Burn, Linfa).
3. **Internal admin / back-office** — tenant management, rule-corpus editor, audit-log browser, lineage explorer, federated-learning coordinator dashboard, proxy-audit dashboard, disparate-impact reports, partner-API token management. Django Admin gives most of this for free.
4. **Customer-facing product UI** — the case workspace with 16+ panels, counterfactual sliders, Monte Carlo distribution charts, Cravath-grade PDF memo export. Next.js + React + Tailwind + shadcn/ui is the modern web stack.

A monolingual architecture (e.g., pure Python) would force expensive rewrites for the data plane and lose compile-time compliance guarantees. A pure-Rust architecture would force years of ecosystem-rebuild for ML/NLP/Logic. Each language's ecosystem strength dictates which work it owns.

## Decision

We adopt a **four-language polyglot architecture with a single gRPC contract boundary**:

- **Rust data plane** owns: API gateway (axum + async-graphql), feature store + compliance enforcement (sqlx + Tier/Sensitivity/PermittedUse ADTs), ingestion fetcher, feature deriver, real-time event broker (tokio + WS), Monte Carlo simulation engine (rayon + ndarray), distributional cost engine (statrs + nalgebra), decision-arithmetic core (pure functions over distributions), partner gateway (separate process from main gateway).
- **Python ML plane** owns: ml-inference-svc (FastAPI), ml-training-job (Airflow / Argo Workflows on GPU pool), llm-client-svc (Gemma 4 client with httpx pool + retry + circuit breaker), nlp-svc (spaCy + Legal-BERT + BERTopic + scikit-fuzzy + modAL), logic-svc (Z3 + pyDatalog / Clingo + owlready2 + py_arg + pyDS + PM4Py), graph-svc (PyG + DGL + HGT + R-GCN + TGN + GraphSAGE + RotatE/ComplEx + NetworkX/cuGraph centrality), personality-svc (LIWC + LoRA), causal-inference-svc (DoWhy + EconML + CEVAE), federated-learning coordinator (Flower + Opacus DP-SGD), simulation orchestrator (Python orchestrating Rust hot loops).
- **Django admin** owns: internal back-office app — tenant mgmt, rule-corpus editor, argumentation-framework editor, audit log, lineage explorer, FL coordinator dashboard, proxy-audit dashboard, disparate-impact reports, partner-API token mgmt. Schema-readonly on Postgres; mutations route through gRPC to the Rust feature-store so the same compliance enforcement applies regardless of which UI made the change. Staff SSO via OIDC.
- **Next.js + React** owns: customer-facing case workspace, intake flow, recommendation summary, factor breakdown, counterfactual exploration, comparable-case retrieval display, Monte Carlo simulation panel, cost breakdown, lead-attorney + expert-witness optimization views, compliance disclosure, PDF memo preview. Apollo Client + Zustand pure reducers + hooks-only.

Cross-language traffic crosses **gRPC** with `prost` + `tonic` on Rust, `grpcio` + `grpcio-tools` on Python (and the same generated stubs on Django). The `protos/` directory is the single source of truth for every contract; `buf` lints schemas and blocks breaking changes in CI.

## Consequences

### Positive

- **Each language does what it's strongest at.** Rust gets type-system-enforced compliance and free parallelism in the decision-arith and Monte Carlo cores. Python keeps the entire ML/NLP/Logic ecosystem unchanged. Django Admin saves weeks of bespoke admin-screen work. Next.js gives modern frontend ergonomics.
- **Compile-time compliance guarantees.** Rust ADTs reject Tier-C protected-class flow into ML/GNN/Decision callers at build time, not runtime. The most expensive class of compliance bugs cannot ship.
- **Free parallelism in functional-core crates.** rayon's `par_iter` works because there's no shared mutable state. Monte Carlo trajectories run 10–100× faster than `simpy` (Python) without thread-safety bookkeeping.
- **Honest ecosystem alignment.** No fighting any language to do what its competitors do better.

### Negative

- **Three primary skill bands required** (Rust + Python + TypeScript). Hiring is harder than monolingual; one Rust engineer is the structural addition (already in the team plan, §16). Pair programming + ADR-FP-001 onboarding doc mitigate.
- **Cross-plane debugging crosses language boundaries.** Mitigated by OpenTelemetry distributed tracing across both planes with correlation IDs threaded through every gRPC call.
- **Schema drift between Rust and Python sides.** Mitigated by `buf lint` + `buf breaking` CI gates and cross-plane integration tests on every `.proto` change.
- **CI compile times are longer than monolingual.** `sccache` + `cargo nextest` + parallel CI workers cap clean-build CI at ≤ 15 minutes.

### Neutral / mitigations

- **Reversibility:** the gRPC boundary lets any service migrate across the boundary later without architectural rewrite. Replace a Python service with a Rust one (or vice versa) by re-implementing against the same `.proto` and switching ArgoCD's deployment manifest. Not free, but not a one-way door.
- **Operational overhead** of running two language runtimes side-by-side: shared mono-repo CI, shared observability stack, shared GitOps controller (ArgoCD App-of-Apps), per-language ops runbooks documented in `runbooks/`.

## Alternatives considered

### Alternative A — Pure Python (FastAPI + Django + Next.js)
**Rejected.** Loses compile-time compliance guarantees on the Tier-C feature-store. Python `mypy` strictness is opt-in and runtime-erasable; a typo or refactor can leak Tier-C data into a model training pipeline at runtime. For a product whose Title VII / disparate-impact exposure is rated Severe in the risk register (§18), runtime-only type-safety is too weak. Also surrenders the 10–100× Monte Carlo speedup that makes the simulation feature interactive instead of batch.

### Alternative B — Pure Rust (Axum + Burn + Linfa + ?)
**Rejected.** The ML/NLP/Logic ecosystem we depend on is Python-first by years. Rewriting CEVAE, owlready2, Z3 bindings, Flower, PySyft, scikit-fuzzy, BERTopic, MAPIE, pyAgrum, etc. in Rust is a multi-year project, not a Phase-1 architectural choice. We would ship without the ML capability the product is built around.

### Alternative C — Two languages only (Rust + Python, no Django, no Next.js)
**Rejected on the back-office side.** The back-office app is mostly CRUD + list-detail-edit-history flows over typed records. Building these from scratch in React + Rust gateway is weeks of bespoke screen work for low-leverage views. Django Admin scaffolds them in hours each. The cost of adding Django (one role, schema-readonly mode, OIDC setup) is substantially less than the cost of building the same screens twice.

### Alternative D — Three languages (Rust + Python + Next.js, no Django)
**Considered seriously.** The back-office screens could in principle be Next.js routes against the same Rust gateway. Rejected because (a) Django Admin's `inspectdb` + unmanaged models pattern means we don't pay schema-ownership cost; (b) the staff SSO surface (OIDC for employees) is genuinely separable from customer JWT and Django's auth tooling is mature; (c) building admin screens in React costs roughly 6× more time than equivalent Django Admin scaffolding. Phase 2 may revisit if the back-office needs grow beyond Django's natural fit.

## Compliance and verification

- **CI gate:** every `.proto` change runs `buf lint` + `buf breaking` + cross-plane integration tests. Schema drift is caught at PR time.
- **Type-system enforcement (Rust side):** the `feature-store-types` crate exposes `Tier`, `Sensitivity`, `PermittedUse` newtype wrappers with exhaustive `match` on Tier-C variants. Only call sites that explicitly accept Tier-C can read those values; everywhere else, a Tier-C value is a compile error. Property tests in `feature-store-types` assert these invariants cannot be circumvented.
- **Type-system enforcement (Python side):** Pydantic models on the gRPC client side mirror the Rust types via generated stubs; runtime validation rejects out-of-tier flow.
- **Code review:** PRs that add a service must declare which plane it belongs to and why. The repo's `CODEOWNERS` enforces appropriate reviewers per plane.
- **Observability:** distributed traces show every cross-plane call with correlation IDs; `gRPC contract-error rate` is a per-service SLO.

## References

- `judicialpredict-v2-spec.md` §7 (Technical Architecture)
- `judicialpredict-v2-spec.md` §11.6.7 (Pragmatic Functional Programming)
- `judicialpredict-v2-spec.md` §5 (Demographic, Personality & Compliance Framework — drives the type-system enforcement requirement)
- ADR-002 (gRPC contracts as single source of truth) — to be authored next
- ADR-003 (Multi-tenant isolation strategy) — to be authored
- ADR-004 (Compliance feature-tier enforcement at Rust type-system boundary) — to be authored
- ADR-FP-001 (functional-core / imperative-shell paradigm boundaries) — to be authored

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*

*Note on authorship: this ADR was seeded by the PM (acting on owner instruction) when the gigforge-engineer agent stalled at the tool-use step on the available free-tier model. The engineer is to review, amend with implementation specifics drawn from §7 and §11.6.7, and re-author the file with their own analysis. The seed is preserved in git history for context.*
