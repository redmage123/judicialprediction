# ADR-FP-001: Functional core, imperative shell — paradigm boundaries per service

**Status:** Accepted
**Date:** 2026-05-07
**Author:** PM-authored from spec §11.6.7; engineer to review
**Reviewers:** gigforge-engineer, gigforge-dev-backend, gigforge-dev-ai, gigforge-qa
**Spec references:** §11.6.7 (Pragmatic Functional Programming — concrete commitments), §11.6.5 (SOLID), §11.6.6 (DRY)
**Plane issue:** JP-2

> This ADR makes the per-service paradigm choices in spec §11.6.7 binding and reviewable. Future paradigm changes require an updated ADR; ad-hoc OOP-in-Rust or imperative-in-functional-core is a code-review block.

## Context

The codebase spans Rust, Python, TypeScript, and Django — four languages, three primary paradigms (FP-leaning Rust, multi-paradigm Python, hooks-based functional React). Without an explicit per-service paradigm rule, three failure modes appear within months in real systems of this size:

1. **Java-in-Rust.** Engineers used to OOP write Rust services with `Rc<RefCell<Box<dyn Trait>>>` mutable-shared-state patterns that fight the borrow checker, produce confusing bugs, and forfeit Rust's compile-time guarantees.
2. **Imperative Python everywhere by default.** Pure-function discipline never takes root; tests become hard to write because every dependency is implicit and stateful; refactors break unrelated code.
3. **Class components in React.** Mutable state scattered across the workspace; time-travel debugging is impossible; reducer-driven UIs become unpredictable.

The cure is **explicit per-service designation**: which services are functional-core (pure, referentially transparent, property-testable), which are functional-leaning (mostly pure, state at the boundaries), and which are imperative because state is genuinely the point.

Spec §11.6.7 enumerates these designations. This ADR makes them binding and CI-checkable.

## Decision

Three paradigm tiers, applied per service / crate / module:

### Tier 1 — Functional Core (binding)

**Rule:** zero mutable global state; every public function is pure given its inputs; rayon parallelism trivially safe; property-tested with proptest (Rust) or hypothesis (Python) for algebraic invariants.

| Service / crate | Why it's functional-core |
|-----------------|-------------------------|
| `rust/decision-arith` | EV / CVaR / Nash / Rubinstein / Kalai-Smorodinsky / prospect-theory utility — pure functions over distributions |
| `rust/monte-carlo-sim` | Pure `(seed, params) -> Trajectory` closures; rayon `par_iter` over N seeds |
| `rust/cost-engine` | Distributional composition is pure: `(component-distributions, correlation-matrix) -> total-distribution` |
| `rust/feature-store-types` | `Tier`, `Sensitivity`, `PermittedUse` newtypes + ADTs; compile-time enforcement |
| `python/logic-svc/rules` | Pure rule application: `(facts, rules) -> derived_facts` over immutable fact bases (frozendict / Pyrsistent pmap) |
| `python/causal-inference-svc/estimators` | DoWhy / EconML / CEVAE wrapped as pure: `(features, treatment, outcome) -> ATE_estimate + CI` |
| `python/nlp-svc/fuzzy-mfs` | `(facts, mf-spec) -> membership_score` |
| `python/ml-inference-svc/conformal` | MAPIE wrapped pure: `(model, calibration_set, x) -> prediction_interval` |

**CI enforcement:** these crates / modules carry a `// FUNCTIONAL-CORE` marker comment at the top of each file. A linter rule rejects:
- `static mut` (Rust)
- `lazy_static!` with mutable contents (Rust)
- module-level mutable state (Python)
- I/O calls (filesystem, network, time, randomness without seed) inside Tier-1 code paths
- `unsafe { }` blocks (Rust) — must be pushed to imperative shell

**Property-test coverage:** Tier-1 modules require ≥ 1 property test asserting an algebraic invariant per public function. CI fails if the proptest / hypothesis count drops.

### Tier 2 — Functional-leaning (recommended; not strictly enforced)

**Rule:** mostly pure transformations; state lives at well-isolated I/O boundaries; immutable updates preferred (Immer in TS; immutable Polars LazyFrame chains in Python; iterators in Rust).

| Service / module | Style |
|------------------|-------|
| `rust/api-gateway` | Request handlers are mostly pure `Request -> Response`; tokio runtime is the imperative shell; auth + rate-limit state injected as dependency |
| `rust/partner-gateway` | Same pattern |
| `python/ml-inference-svc/serving` | Serving is `(features) -> prediction` modulo loaded weights; multi-task heads compose as function composition |
| `next.js/workspace` | Zustand or Redux Toolkit pure reducers; no class components; hooks-only; immutable state via Immer |
| `python/*/data-pipelines` | Polars LazyFrame for immutable transformation chains; never `df.column = ...` |
| `rust/api-gateway/resolvers` (GraphQL) | Pure when data is cached or read-only; side-effect-isolated otherwise; DataLoader composes purely |

**No CI enforcement** beyond the existing lint suite (no class components rule for React, no mutable Pandas operations preferred over Polars LazyFrame in PR review).

### Tier 3 — Imperative / OOP (allowed because state is the point)

**Rule:** state is genuinely intrinsic; clear-and-honest beats forced-pure.

| Service | Why imperative is correct |
|---------|--------------------------|
| `python/federated-learning-coord` | Round counters, privacy budgets, tenant participation history are intrinsically stateful |
| `python/ml-training-job` | Model weights evolve; epoch state is real |
| `python/ml-model-registry` | MLflow is stateful by design |
| Django ORM, SQLAlchemy, sqlx | Relational state is the point |
| `rust/ingest-fetcher` | I/O orchestration with checkpointing |
| `rust/event-broker` | Connection-state management |
| `python/logic-svc/defeater-bookkeeping` | Order of rule application matters; mutable working memory is honest about what's actually happening (the *individual rules* remain pure functions; only the *engine* that schedules them is imperative) |

## Boundary rules (universal)

- **Effects at the edge.** Network calls, DB writes, file I/O, time, randomness — all explicit at service boundaries; never buried in nominally pure logic.
- **Pure cores composed via simple data structures.** Cross-module integration uses plain data (records, sum types, immutable collections), never shared mutable state.
- **No monad-transformer towers.** This is Rust + Python + TypeScript, not Haskell. Pragmatic FP, not category theory.
- **Don't fight the language.** Rust naturally rewards FP idioms; Python tolerates them with `frozendict` / Pyrsistent; TypeScript is happy with hooks + reducers. Stay in idiomatic-FP for each language; never write Haskell-in-Python.

## Consequences

### Positive

- **Property-based testing is dramatically more effective** on pure functions. Hypothesis / proptest find counterexamples to algebraic invariants the unit tests miss.
- **Free parallelism in Tier-1 services.** rayon's `par_iter` works because there's no shared mutable state to coordinate. The Monte Carlo engine's 10–100× speedup over simpy is partly Rust, partly the trivially-parallel pure-function shape.
- **Compile-time compliance enforcement.** Rust ADTs reject Tier-C-flow violations because the `Tier::C` ADT variant cannot satisfy a `PermittedUse` bound that excludes it. Pure types over mutable state.
- **Replay and audit are honest.** Pure functions take their inputs explicitly; given the same inputs, they produce the same outputs. This is the core of reproducible legal analysis — a recommendation can be re-derived from the recorded inputs years later.
- **Reasoning chains compose cleanly.** Layer 2's argumentation defeats compose with Layer 1's conformal intervals through pure-function plumbing; the imperative state lives only at the I/O edges.

### Negative

- **Rust learning curve for engineers used to OOP.** Mitigated by pair programming + this ADR as onboarding doc + code-review feedback specifically targeting "are you fighting the language?".
- **Some Python developers will want to write classes everywhere.** Code review enforces the boundary; the rule is "use a class when state is genuine, otherwise use a function."
- **Debugging deeply-nested functional pipelines requires different tools.** Polars `.collect_schema()` + lazy-plan introspection; Rust `tracing::instrument` spans on functional-core entry points; OpenTelemetry distributed tracing across both planes. Investment from week 1 (§11.5).
- **Pure-function discipline can produce verbose code** in Python compared to mutable shortcuts. Accept the verbosity; correctness on legal-prediction code matters more than line count.

### Neutral / mitigations

- **Reversibility:** moving a service from Tier 1 to Tier 2 (or vice versa) is a routine refactor — there's no architectural lock-in here. The ADR captures intent; reality wins where intent and reality disagree.
- **Edge cases:** when a Tier-1 service genuinely needs an effect (e.g., logging), the effect is pushed to an injected logger trait that the function takes as a parameter; the function remains pure given that parameter, and the imperative `tracing::Subscriber` lives in the shell.

## Alternatives considered

### Alternative A — Pure-FP throughout (Haskell-style)
**Rejected.** Wrong language ecosystem; would force monad-transformer stacks in Python that no one will read willingly; would lose the Python ML ecosystem (PyTorch, scikit-learn, PyMC are imperative-by-API). Pragmatic FP wins because we don't have to fight library design.

### Alternative B — OOP-everywhere by default
**Rejected.** Loses the property-testability of the Tier-1 services; loses free parallelism in the Monte Carlo + decision-arith hot paths; loses compile-time compliance enforcement (mutable type state is harder to reason about); makes the reasoning-layer composition (§8) significantly noisier.

### Alternative C — Per-service paradigm with no central rule
**Rejected.** Drift is inevitable. Without an explicit ADR + CI markers + code-review enforcement, "functional core" decays to "mostly imperative with some pure functions" within two sprints. The whole point is an architectural rule that survives turnover and reviewer-reviewer disagreement.

## Compliance and verification

- **CI markers:** Tier-1 files start with `// FUNCTIONAL-CORE` (Rust) or `# FUNCTIONAL-CORE` (Python). A linter rule (added to the standard `cargo clippy` + `ruff` configs) rejects forbidden constructs in marked files.
- **Property-test count gate:** CI tracks proptest / hypothesis test count per Tier-1 module; PRs that drop the count below baseline require an explicit acknowledgement in the PR description.
- **Code review checklist** (added to the PR template):
  - For new Rust code: am I using iterators + `match` + `Option`/`Result` idiomatically, or am I writing Java-in-Rust?
  - For new Python code: should this be a function instead of a class? Is the state genuinely intrinsic?
  - For new React code: is this a function component with hooks, or did I forget?
- **Onboarding:** every new engineer reads this ADR + spec §11.6.7 within their first sprint. Pair programming on the first ADR-touched task.
- **Architectural review:** quarterly review of paradigm decisions; any decay (Tier-1 modules accumulating mutable state) is flagged as tech debt and added to the backlog.

## References

- `judicialpredict-v2-spec.md` §11.6.7 (Pragmatic Functional Programming — concrete commitments)
- `judicialpredict-v2-spec.md` §11.6.5 (SOLID per language)
- `judicialpredict-v2-spec.md` §11.6.6 (DRY with rule-of-three)
- ADR-001 (Polyglot architecture boundary)
- ADR-002 (gRPC contracts as single source of truth)
- "Functional core, imperative shell" — Gary Bernhardt, "Boundaries" (2012). The original framing.
- Rust API Guidelines (https://rust-lang.github.io/api-guidelines/) — idiomatic FP-leaning Rust patterns.

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*
