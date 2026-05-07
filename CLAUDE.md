# CLAUDE.md — JudicialPredict

This is the JudicialPredict mono-repo. Specification stage; implementation has not started.

## Source of truth

- [`.project-docs/judicialpredict-v2-spec.md`](.project-docs/judicialpredict-v2-spec.md) — Software Specification & Project Plan (current version v2.13)
- [`.project-docs/judicialpredict-wireframes.md`](.project-docs/judicialpredict-wireframes.md) — Wireframes + IA + state catalogue

When working on this repo, read those two documents first. The README is a summary; the spec is normative.

## Architecture invariants

Polyglot Rust + Python + Django + Next.js:

- **Rust data plane** — API gateway, feature store + compliance enforcement, Monte Carlo simulation, ingestion, real-time event broker, decision-arithmetic core, partner gateway. Functional-core for `decision-arith`, `monte-carlo-sim`, `cost-engine`, `feature-store-types`.
- **Python ML plane** — ML inference, training jobs, NLP, graph ML, logic, personality, federated learning, causal inference. Functional-leaning where state isn't genuine; stateful services (FL coordinator, training jobs, model registry, ORM) honestly imperative.
- **Django admin** — internal back-office app. Schema-readonly on Postgres; mutations route through gRPC to Rust feature-store.
- **Next.js workspace** — customer-facing app. Zustand or Redux Toolkit pure reducers; no class components; hooks-only.
- **gRPC** between the planes. `prost`/`tonic` on Rust ↔ `grpcio` on Python. Schemas in `protos/` as single source of truth.

## Compliance hard rules

- Tier-C protected-class features cannot enter ML / GNN / NLP / Decision predictive paths. Compile-time enforcement in the Rust feature-store.
- Federated learning is opt-in per tenant; DP guarantees published.
- All recommendations carry SHAP + GAT-attention factor breakdown + rule trace + comparable-case list. No black-box outputs.

## Non-negotiable methodology

TDD (red-green-refactor), BDD (Gherkin + Three Amigos), pair programming default, trunk-based development, conventional commits, ≥1 reviewer per PR (≥2 for compliance-touching changes), property-based testing on functional-core crates, mutation testing weekly, accessibility WCAG 2.2 AA from day one.

## Phase 1 deferrals (do not implement without explicit scope change)

Voir-dire / jury-selection support (parked behind ethical-review gate); RLHF on recommendation thresholds; mobile native apps; multi-jurisdiction beyond Federal + CA + NJ; non-English locales; Dark Triad / IAT profiling.
