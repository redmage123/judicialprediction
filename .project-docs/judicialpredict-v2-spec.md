# JudicialPredict v2.14 — Software Specification & Project Plan

**Document Type:** Software Specification & Project Plan
**Version:** 2.14 (Re-scoped from GigForge v1.0 of 25 March 2026; expanded through v2.0 → v2.1 → v2.2 → v2.3 → v2.4 → v2.5 → v2.6 → v2.7 → v2.8 → v2.9 → v2.10 → v2.11 → v2.12 → v2.13 → v2.14)

> v2.14 is a cleanup pass, not a feature addition. The earlier "(NEW HIRE)" framing inherited from the GigForge v1.0 consultancy spec has been replaced in §16 with the actual gigforge agent assignments. The work itself is unchanged. The Phase-1 wall-clock projection is recalibrated to 8–16 weeks (vs the original 41) since agent capacity is sub-day per story; remaining gating dependencies are real-world-bound (cloud account, pilot-firm scheduling, Legal-SME sign-off).
**Companion document:** `judicialpredict-wireframes.md` (low-fidelity wireframes + IA + state catalogue + accessibility checklist + performance budgets + voice & tone guide)
**Date:** 7 May 2026
**Status:** Draft for Client Review
**Classification:** Confidential

> v2.13 makes the **functional-programming-where-it-fits** commitments concrete per service rather than aspirational. Specific designations: the **Rust decision-arithmetic core** is a pure-function library (zero global state, referentially transparent, rayon-parallelizable, property-tested as algebraic invariants); the **Rust Monte Carlo simulation engine** is functional-core (pure trajectory-generation closures + imperative shell for I/O); the **Rust feature store** uses ADT + exhaustive-match for compile-time tier enforcement; the **Python logic service** is implemented as pure rule-application functions over immutable fact bases with effects isolated at I/O boundaries; the **ML data pipelines** standardize on Polars LazyFrame for immutable transformation chains; the **Python causal-inference layer** runs as pure estimator functions; the **Next.js workspace** uses Zustand (or Redux Toolkit) pure reducers for state, no class components, hooks-only; the **GraphQL gateway** resolvers are pure where data is cached and side-effect-isolated otherwise. Stateful services (federated-learning coordinator, training jobs, model registry, ORM layers, ingestion fetchers) explicitly retain imperative / object-oriented styles where state is the point. Adds an **Architectural Decision Record (ADR-FP-001)** to the deliverables documenting the per-service paradigm choices and the boundary rules.

> v2.12 promotes engineering methodology to a first-class discipline (§11.6) — two-week Agile sprints with Scrum ceremonies, full XP practices (pair programming default, trunk-based development, sustainable pace, YAGNI, continuous refactoring), TDD as the default development cycle (red-green-refactor) with mutation testing weekly, BDD with Gherkin acceptance criteria + Three Amigos sessions + living documentation via Docusaurus, SOLID principles applied per-language with judgment (not dogma), DRY with rule-of-three discipline (preferring three clear lines over a premature abstraction), pragmatic functional programming (functional core, imperative shell — full FP for the Rust decision-arithmetic core, the logic service, and frontend state; OOP/imperative where state genuinely belongs), full DevOps culture (DORA metrics from week 1, SLO/SLI/error-budget gating, blameless postmortems, weekly chaos experiments via LitmusChaos starting week 16, quarterly game days), and explicit code-review standards (≥1 reviewer per PR, ≥2 for compliance-touching changes, Legal SME sign-off for rule-engine changes, 4-hour review SLA). Sprint cadence drives the existing 41-week timeline; Scrum Master role is added to PM Jamie Okafor's responsibilities.

> v2.11 promotes UI/UX from "frontend stack" to a first-class **design discipline** — design system + component library, user research + personas, information architecture, data visualization design language, WCAG 2.2 AA accessibility, print/PDF memo design, onboarding flows, empty/loading/error states, performance budgets, product analytics + feature flags, brand identity, tablet-responsive design, i18n scaffolding (English-only Phase 1), microinteractions / motion design, and a documented voice & tone guide. Companion `judicialpredict-wireframes.md` carries the low-fidelity wireframes for the major flows. Adds Senior Product Designer, UX Researcher (0.5 FTE), Accessibility Consultant (0.25 FTE), and an additional design-system-focused frontend engineer. Phase 1 timeline extends to ~41 weeks (UX research and design system run in parallel weeks 1–8 but shift frontend implementation milestones forward by ~3 weeks).

> v2.10 adds a **quantum / quantum-inspired sub-layer simulated on classical hardware** (no real QPU dependency): (1) **tensor networks** (quimb + tntorch) — quantum-inspired classical algorithms for BDN inference scaling and high-order feature-interaction compression in the gradient-boosted ensemble; (2) **quantum kernel methods** (PennyLane on simulator, 4–8 qubits) as ensemble members alongside XGBoost/LightGBM/CatBoost, contributing different feature-space geometry; (3) **QAOA on simulator** for three operational combinatorial problems — discovery scheduling, expert-witness assignment, motion prioritization; (4) **quantum walks on the citation graph** as additional centrality features alongside PageRank/betweenness/Louvain; (5) **VQC as ensemble member** at small qubit counts (4–12); (6) **amplitude-estimation-inspired classical importance sampling** for the Monte Carlo trial simulation engine, accelerating tail-distribution estimation 2–5×. Stack additions: PennyLane, Qiskit + Qiskit Aer (with NVIDIA cuQuantum GPU acceleration), quimb, tntorch — all run on existing `general-pool` and `gpu-pool` node pools, no new hardware. Realistic value framing: not raw speedup (simulated quantum is generally slower than equivalent classical on classical hardware) but ensemble diversity, different inductive biases, R&D credibility, and quantum-ready architecture for when real QPUs mature. ~3–4 weeks of additional Phase 1 work; no new hires (existing ML + Graph specialists; recruiting flag for PennyLane / Qiskit experience).

> Supersedes v1.0 through v2.8. v2.9 deepens the psychological-methodology stack with six well-grounded additions: (1) **Moral Foundations Theory (MFT)** — care/fairness/loyalty/authority/sanctity/liberty as Tier-A judge features predicting morally-laden case outcomes; (2) **Cognitive bias profiling** — per-judge anchoring, status-quo, hindsight, in-group bias scores derived from prior decisions (Guthrie/Rachlinski/Wistrich research base); (3) **HEXACO** — supplements Big Five with Honesty-Humility for witness credibility and ethical-tactic discrimination; (4) **Prospect theory in the decision layer** — loss-aversion + reference-point utility curves replacing implicit symmetric utility; (5) **Negotiation psychology** — anchor-and-adjust first-offer dynamics, framing effects, explicit BATNA/WATNA/ZOPA modelling augmenting Nash + Rubinstein; (6) **Procedural justice theory (Tyler)** — multiplier on settlement-acceptance probability beyond pure economic value. Plus the lighter Tier-2 wins: cognitive-style profiling, GDMS, Cialdini persuasion-style, career-trajectory personality drift via TGN, conservative witness credibility scoring, stress/pressure response from temporal opinion data. Dark Triad / IAT / jury-psychology remain excluded (ethical-review parking lot). Adds no new headcount — existing NLP/Personality + Causal + ML specialists own the work; ~2–3 weeks of additional Phase 1 timeline.

> Supersedes the earlier short note: **v2.8** specified the Kubernetes + GitOps platform (shared-cluster topology, operators, ArgoCD App-of-Apps with promotion-as-PR, Argo Rollouts metric-gated canary, Trivy/Syft/Cosign image hardening, Prometheus/Grafana/Loki/Tempo observability) and added the Senior SRE / Platform Engineer to the team.

---

## 1. Executive Summary

JudicialPredict is an analysis-only case-evaluation platform for US law firms. Given a case file, it estimates the probability of success at trial, the distribution of damages or sentencing outcomes, the cost and duration of litigation, and the settlement-value range — and returns a defensible **settle / try / borderline** recommendation with the reasoning the firm can show a client.

The system is built as a **polyglot architecture**:

- **Rust data plane** — API gateway, feature store + compliance enforcement, Monte Carlo simulation engine, distributional cost engine, ingestion pipelines, real-time event broker, decision-arithmetic core. CPU-bound or high-concurrency work where Rust's compile-time guarantees and runtime performance materially benefit the product.
- **Python ML plane** — ML training/inference, NLP, graph-ML, logic services, LLM client, federated-learning coordinator, causal inference, personality / topic / LoRA services. Where the Python ecosystem (PyTorch, scikit-learn, PyMC, spaCy, Hugging Face, DoWhy, owlready2, Z3, Flower) is deeply established and Rust equivalents would force years of ecosystem rewrite.
- **Django admin / back-office app** — platform-admin and tenant-admin surfaces (tenant management, rule-corpus editor, audit-log browser, feature-store metadata browser, lineage explorer, proxy-audit dashboard, federated-learning coordinator dashboard, disparate-impact reports, partner-API token management). Schema-readonly on Postgres; mutations route through gRPC to the Rust feature-store so compliance enforcement is centralised.
- **gRPC contract** between the planes — `prost`/`tonic` on Rust, `grpcio` on Python (and Python-Django), schemas in `.proto` as single source of truth.

The reasoning stack remains four layers (Probabilistic/ML, Logic, NLP+Fuzzy, Decision/Action) plus the heterogeneous knowledge graph, the demographic/personality/compliance framework, the Monte Carlo trial simulation engine, the federated-learning + differential-privacy infrastructure, and a **generative + latent-variable sub-layer** for imputation, synthetic-data sharing, latent-confounder causal inference, and cross-jurisdiction adversarial training. The partner API enables integration with Clio, MyCase, and NetDocuments.

All data sources are free / open-licensed. Fine-tuning runs on the existing Gemma 4 inference pod with two LoRA adapters: `judicialpredict-en` and `personality-en`.

## 2. Project Overview & Objectives

### Vision

To be the law firm's go-to second opinion on whether to settle or try a case — with the math, the law, and the reasoning shown end-to-end — built on an architecture that is honest about performance (Rust where it matters), honest about ecosystem (Python where the science lives), and honest about confidentiality (federated learning + DP for shared models, hard tier-enforcement for protected-class features).

### Primary Objectives

(unchanged from v2.3 — calibrated P(win), distributional outputs, conformal coverage, settle/try recommendations, statutory-rule application, multi-tenancy, both-sides framing, tiered demographic features, lead-attorney + expert-witness optimization, federated learning, partner integrations).

### Non-Objectives (Phase 1)

- No prediction-market / wagering / real-money functionality.
- No mobile native apps.
- No Croatian or other non-English language support.
- No paid data sources.
- No automated court e-filing integration.
- No legal advice.
- No predictive use of party-level protected-class features outside narrow element-required cases.
- No voir-dire / jury-selection features (Phase 2 with explicit ethical-review gate).
- **No pure-Rust ML services** — the ML / NLP / logic ecosystem is Python-first; rewriting it in Rust is not a Phase 1 objective.
- **No pure-Python data plane** — performance-critical and compliance-critical hot paths run in Rust.

## 3. Scope of Work

### Jurisdictional Coverage

| Tier | Jurisdiction | Civil | Criminal | Bankruptcy |
|------|--------------|-------|----------|------------|
| Federal | US Code (Titles 11, 15, 18, 28, 42, etc.) + Federal Rules | ✓ | ✓ | ✓ (Title 11, exclusive) |
| State | California (Civil, CCP, Penal, Evidence Codes) | ✓ | ✓ | — |
| State | New Jersey (Title 2A, 2C, court rules) | ✓ | ✓ | — |

### Substantive Law Coverage

Contract (CA + NJ + UCC + federal); Tort (CA + NJ — negligence, intentional torts, products liability, defamation, fraud); Bankruptcy (Title 11 — Chapter 7, 11, 13); Criminal — federal (Title 18, Sentencing Guidelines); Criminal — CA (Penal Code, three-strikes, determinate); Criminal — NJ (Title 2C, Graves Act, NERA).

### Both-Sides Framing

- **Civil — plaintiff seat:** P(liability), expected damages distribution, fee-recovery probability, settlement floor.
- **Civil — defendant seat:** P(liability), exposure distribution, defense cost, settlement ceiling, CVaR.
- **Criminal — defense seat:** P(conviction by charge), expected sentence, plea-vs-trial EV and CVaR.
- **Criminal — prosecution seat:** P(conviction), charge-bargaining, declination signals.

## 4. Data Sources & Ingestion

All sources free / open-licensed. Adapter-layered so paid sources can plug in later. **Ingestion pipelines run in Rust** (high-throughput parsing, `serde` + `csv` + `flate2` + `tokio` for concurrent fetches) for the data-movement layer, with Python orchestration via Airflow / Prefect for scheduling and downstream ML feature computation.

### Case Law & Dockets

CourtListener / RECAP, Caselaw Access Project, CA Courts, NJ Judiciary.

### Statutes & Rules

Cornell LII, CA Legislative Counsel, NJ Statutes, US Sentencing Guidelines.

### Legal Textbooks & Treatises (open-license fine-tune corpus)

CALI eLangdell, Saylor Academy, OpenStax, FJC publications, state bar CLE.

### Ideology, Demographic & Personality Sources

Martin–Quinn, Judicial Common Space, Bonica DIME, FEC + state campaign-finance, FJC Biographical Directory, state judicial bios, state bar records, public deposition / oral-argument transcripts, AmLaw 100 / Vault, Federalist Society / ACS rosters where public.

### Tabular & Multi-modal Sources

Tabular PDF extraction via Camelot / pdfplumber.

### Academic Partnership Data

Law-school labelled litigation datasets via collaboration agreements.

## 5. Demographic, Personality & Compliance Framework

(unchanged from v2.3 — Tier A judges, Tier B attorneys, Tier C parties, Tier D experts, with feature-store metadata, protected-class proxy audit, per-tenant toggles, lineage tracking, PDF memo disclosure, disparate-impact reports).

**Feature store and compliance enforcement run in Rust** (see §7) — the type system rejects unauthorised Tier-C feature flow at compile time, not runtime.

### 5.1 Tier A — Judges (full feature set)

Ideology scores (Martin–Quinn, JCS, Bonica DIME); appointing authority / path; affiliations; law school + class year; age + tenure + prior career; judicial-temperament scores; **HEXACO personality vector (supersedes Big Five — Honesty / Emotionality / Extraversion / Agreeableness / Conscientiousness / Openness; Big Five recoverable from HEXACO so no information loss)** from authored opinions; **Moral Foundations Theory (MFT) profile — care, fairness, loyalty, authority, sanctity, liberty — derived from opinion text via the Moral Foundations Dictionary 2.0 + Gemma 4 LoRA scorer**; **cognitive bias profile — anchoring susceptibility, status-quo bias, hindsight bias, in-group favoritism — derived from prior-decision patterns following Guthrie / Rachlinski / Wistrich research methodology**; **cognitive style — Need for Cognition + ambiguity tolerance — extracted from authored opinions**; **stress / pressure response signature — how decision quality and length vary with caseload pressure**; **career-trajectory personality drift — TGN-modelled HEXACO + MFT + temperament evolution across career stages**; reversal record by appellate court.

### 5.2 Tier B — Attorneys (mostly public-record, two carve-outs)

**Predictive:** law school + class year + class rank if public; experience + admission + sanctions; firm tier + practice area; public win/loss record; **courtroom HEXACO** (Honesty-Humility especially relevant for ethical-tactic discrimination); **General Decision-Making Style (GDMS — rational / intuitive / dependent / avoidant / spontaneous)** from briefs + transcripts; **Cialdini persuasion style** classification (reciprocity / commitment / social proof / authority / liking / scarcity emphasis); FEC donations as ideology proxy.

**Descriptive only:** gender; race/ethnicity. Audit reports only.

### 5.3 Tier C — Parties (minimal, element-required only)

**Default: hard block on all party-level demographic features from predictive use.**

**Narrow exception:** Title VII / ADA / FHA / ADEA / §1981 / §1983 / ECOA causes of action. Read via `with_protected_class_for_element=true` query argument. **Rust feature-store enforces this at compile time — Tier-C-tagged values cannot flow into ML, GNN, NLP, or Decision callers without the explicit token type.**

**Always blocked from predictive use:** race, ethnicity, national origin, gender, sexual orientation, disability, religion, age (except where age is the cause-of-action element), familial status, marital status, immigration status, veteran status.

### 5.4 Tier D — Expert Witnesses (same rules as Attorneys)

Public CV, prior testimony, fields of expertise; **HEXACO from prior testimony / authored articles (Honesty-Humility especially weighted for credibility)**; **conservative witness-credibility score** combining Daubert reliability factors (testability, peer review, error rate, general acceptance) with linguistic-confidence indicators — surfaces *uncertainty* explicitly, never produces a "this witness is lying" output (deception detection from text is methodologically weak; the score is a Daubert-reliability proxy, not a truthfulness verdict). No protected-class profiling.

### 5.5 Compliance Architecture

- **Feature-store with metadata** — every feature carries `entity_tier`, `sensitivity`, `permitted_uses`. Implemented in Rust; type system enforces permitted-uses mask. Violations are compile errors when possible, hard runtime errors otherwise.
- **Protected-class proxy audit** — periodic SHAP analysis; flagged features go to compliance review.
- **Per-tenant feature toggles** — each firm sets stricter posture beyond global rules.
- **Feature lineage tracking** — provenance for every feature.
- **PDF memo disclosure.**
- **Tenant-level disparate-impact reports** — quarterly.

## 6. Heterogeneous Knowledge Graph

(unchanged from v2.3 — node types, edge types, network-analysis layer with PageRank / betweenness / centrality + Louvain/Leiden community detection, sensitivity tags, Neo4j 5 + pgvector storage, schema versioning).

The Rust feature-store and ingestion services own KG read-write; the Python graph-ML service owns GNN training and embedding production. Cross-plane traffic via gRPC.

## 7. Technical Architecture

### High-Level — Polyglot Rust + Python

```
+-----------------------------------------------------------------------+
|     Next.js 15 + React 19            |    Django Admin / Back-Office  |
|     (Customer-facing app)            |    (Internal admin app)        |
|     Intake | Workspace | Comparables | Tenant mgmt | Rule editor      |
|     Recommendation | Compliance | API| Audit log | Lineage | FL dash  |
+--------------+-----------------------+------------------+-------------+
               | GraphQL + WS                             | gRPC (mutations)
               |                                          | + readonly Postgres
+==============v==========================================v=============+
|        RUST DATA PLANE                                                |
|----------------------------+------------------------------------------|
|  Main API Gateway (customer-facing)                                   |
|    axum + async-graphql + tokio                                       |
|    JWT + RBAC + Tenant + Feature-Tier enforcement + DataLoader        |
|  Partner Gateway (NEW v2.7)                                           |
|    Separate Rust process; OAuth 2 scoped tokens; per-partner          |
|    rate-limit + abuse-monitoring; revocation; blast-radius isolation  |
|  Feature Store + Compliance Enforcement (ADTs + exhaustive match)     |
|    sqlx + Postgres; tier/sensitivity types in the type system         |
|    Tier-C flow rejected at compile time, not runtime                  |
|  Ingest Fetcher (NEW v2.7 split)                                      |
|    serde + reqwest + flate2; raw download + checksum + S3 store       |
|  Feature Deriver (NEW v2.7 split)                                     |
|    rayon + serde; replays from ingested raw blobs without re-fetch    |
|  Real-time Event Broker                                               |
|    tokio + WebSocket fan-out + Redis Streams                          |
|  Monte Carlo Simulation Engine (functional-core, pure trajectories)   |
|    rayon + ndarray + nalgebra                                         |
|    Pure (seed, params) → trajectory closure; embarrassingly parallel  |
|  Distributional Cost Engine                                           |
|    nalgebra + statrs                                                  |
|  Decision-Arithmetic Core (functional-core, pure)                     |
|    EV + CVaR + Nash + Rubinstein inner loops in Rust                  |
|    Zero global state · rayon parallel · property-tested invariants    |
+============================+==========================================+
                             | gRPC (prost ↔ grpcio) + .proto contracts
+============================v==========================================+
|        PYTHON ML PLANE                                                |
|--+----------+----------+--+-------+--------+--------+--------+--------+
|  ML Inference| ML Train | Logic   | NLP/   | LLM     | Graph  | FedL  |
|  Service     | Job      | Svc     | Fuzzy  | Client  | Svc    | Coord |
|  (NEW split) | (NEW)    | Z3+     | Svc    | (NEW    | PyG/   | Flwr  |
|  FastAPI     | Airflow/ | Datlg+  | spaCy+ | split)  | DGL    | +DP-  |
|  XGB/LGB/CB  | Argo     | OWL+Arg | scikit-| Gemma 4 | HGT/   | SGD   |
|  PyMC/NumPyro| GPU pool | +DSh+   | fuzzy+ | client  | TGN+   | +Sec  |
|  MTL backbone|          | ASP+    | BERTop+| +pool   | Cent+  | Aggr  |
|  BDN/MAPIE   |          | Tempor+ | modAL+ | +retry  | Commun |       |
|  Counterfact |          | Deontic+| Camel+ | +rate   | +KGEmb |       |
|  Causal/DoWhy|          | ProcMin | SelfCon|         |        |       |
|  Surv/Drift  |          |         |        |         |        |       |
|  β-VAE/CEVAE |          |         |        |         |        |       |
+--------------+----------+---------+--------+---------+--------+-------+
| Personality Service | Decision Orchestrator                           |
| LIWC + Gemma LoRA   | EV/CVaR/Nash/Rubinstein/SDP/BDN orchestrator    |
+---------------------+-------------------------------------------------+
                             |
+----------------------------v------------------------------------------+
|  Postgres 16 + pgvector  |  Neo4j 5  |  Redis 7  |  MinIO             |
|  Feature Store w/ tier + sensitivity + lineage + drift metadata       |
+-----------------------------------------------------------------------+
                             |
+----------------------------v------------------------------------------+
|   Gemma 4 Inference (existing RunPod 194.68.245.154:22181)            |
|   Base + judicialpredict-en LoRA + personality-en LoRA                |
+-----------------------------------------------------------------------+
                             |
+----------------------------v------------------------------------------+
|        Partner API (Rust gateway): Clio, MyCase, NetDocuments         |
+-----------------------------------------------------------------------+
```

### Stack — Rust Data Plane

| Component | Technology |
|-----------|------------|
| Async runtime | `tokio` |
| HTTP | `axum` (or `actix-web` if needs match better) |
| **`partner-gateway`** (v2.7 split) | **Separate axum + async-graphql process; OAuth 2 with scoped tokens; per-partner rate-limit + quota; abuse-monitoring; revocation tooling; isolated from main gateway so partner traffic spikes can't degrade customer-facing UX** |
| **`ingest-fetcher`** (v2.7 split) | **Rust binary; raw download + checksum + S3/MinIO storage; idempotent + replayable; emits `feature-deriver` jobs via Redis Streams** |
| **`feature-deriver`** (v2.7 split) | **Rust binary; consumes raw blobs from S3, derives features into Postgres + pgvector; can replay any window without re-fetching upstream** |
| GraphQL | `async-graphql` + DataLoader pattern |
| gRPC | `prost` + `tonic` |
| Postgres | `sqlx` (compile-time-checked SQL) |
| Neo4j | `neo4rs` (or `cypher-rs`); fall back to HTTP API where the driver is immature |
| Redis | `redis` crate |
| MinIO / S3 | `aws-sdk-s3` or `minio-rs` |
| Serialization | `serde` + `serde_json` + `prost` (proto) |
| HTTP client (ingestion) | `reqwest` + `flate2` for gzip / `zstd` for zstd |
| Concurrency | `rayon` for CPU-bound parallelism; `tokio` for I/O concurrency |
| Numerics | `ndarray`, `nalgebra`, `statrs` |
| Monte Carlo / discrete-event sim | custom on `rayon` + `ndarray` (replaces `simpy`) |
| Tracing | `tracing` + `tracing-subscriber` + OpenTelemetry exporter |
| Metrics | `metrics` + Prometheus exporter |
| Feature-store types | custom newtype wrappers expressing `Tier`, `Sensitivity`, `PermittedUse` in the type system; flow analysis at compile time |
| Build / CI | `cargo nextest`, `sccache`, parallel workers, dependency caching |

### Stack — Python ML Plane

| Component | Technology |
|-----------|------------|
| Service framework | Python 3.12, FastAPI |
| **`ml-inference-svc`** (v2.7 split) | **FastAPI; horizontally scaled; serves XGB/LGB/CatBoost + PyMC posterior samples + MTL heads + BDN + Conformal + Counterfactuals + β-VAE imputation; sub-100ms p99 target on cached features** |
| **`ml-training-job`** (v2.7 split) | **Airflow / Argo Workflows on GPU pool; long-running batch; nightly retrains, champion/challenger via MLflow, distributional drift triggers; never on the inference critical path** |
| **`llm-client-svc`** (v2.7 split) | **Dedicated FastAPI process for Gemma 4 calls; httpx async pool, exponential-backoff retry, token-bucket rate-limit, circuit breaker (`pybreaker`), prompt-cache layer, request batching where beneficial; one place to manage Gemma 4 SLA** |
| gRPC | `grpcio` + `grpcio-tools` |
| ML | scikit-learn, XGBoost / LightGBM / CatBoost (with monotonic + quantile heads), PyMC / NumPyro, MAPIE (conformal), lifelines (survival), DoWhy / EconML (causal), DiCE / Alibi (counterfactuals), pyAgrum (BDN), **β-VAE (PyTorch) for tabular imputation, TabDDPM with DP for synthetic data, CEVAE (causalml or research impls) for latent-confounder causal inference, pytorch-domain-adaptation for adversarial domain training**, MLflow |
| Graph ML | PyTorch Geometric, DGL, HGT, R-GCN, TGN, GraphSAGE, RotatE / ComplEx / TransE, NetworkX / cuGraph (centrality + community detection) |
| NLP | spaCy 3, Legal-BERT, scikit-fuzzy, Gemma 4 client, modAL (active learning), BERTopic / Tomotopy, self-consistency / CoT verifier, Camelot / pdfplumber |
| Personality | LIWC features, Big Five classifier on Gemma 4 LoRA |
| Logic | Z3 (SMT), Clingo / pyDatalog, owlready2 (OWL/DL), py_arg (argumentation), pyDS (Dempster–Shafer), defeasible logic, PM4Py (process mining) |
| Federated learning | PySyft / Flower / TensorFlow Federated; Opacus (DP-SGD); secure-aggregation primitives |
| Drift detection | River / Alibi Detect / Evidently |
| Multi-task learning | shared PyTorch / scikit backbone with task-specific heads |
| Property-based testing | `hypothesis` (Python side) + `proptest` (Rust side) |

### Cross-plane contract

- **Single source of truth: `.proto` files** in a shared `protos/` directory, versioned alongside the code.
- `prost` codegen on the Rust side; `grpcio-tools` on the Python side; the same `.proto` produces both clients.
- Backwards-compatible evolution rules: additive only without a major-version bump; mandatory CI checks for breaking changes.
- Schema linting via `buf`.

### Frontend

| Component | Technology |
|-----------|------------|
| Framework | Next.js 15, React 19 |
| Styling | Tailwind 4, shadcn/ui |
| GraphQL client | Apollo Client (generated types from the Rust gateway's schema) |
| Subscriptions | GraphQL over WS |
| **Design source of truth** | **Figma** |
| **Component catalogue** | **Storybook + customised shadcn/ui + design tokens (TS-defined)** |
| **Visual regression** | **Chromatic** |
| **Accessibility CI** | **axe-core (per-PR) + Pa11y (monthly) + manual NVDA/VoiceOver passes** |
| **Performance CI** | **Lighthouse CI with budgets enforced** |
| **PDF memo rendering** | **Puppeteer (or WeasyPrint) over dedicated print-stylesheet route** |
| **Product analytics** | **PostHog (self-hosted, PII-masked SDK)** |
| **Feature flags** | **GrowthBook (or Unleash)** |
| **Internationalization** | **next-intl (en-US only Phase 1, scaffolded)** |
| **Motion** | **Framer Motion (respects `prefers-reduced-motion`)** |
| **RUM** | **PostHog Web Vitals** |

### Internal Admin / Back-Office (NEW in v2.6)

| Component | Technology |
|-----------|------------|
| Framework | Django 5 + Django Admin |
| API surface | DRF (Django REST Framework) for any JSON APIs |
| Version history | `django-simple-history`, `django-reversion` |
| Object-level perms | `django-guardian` |
| CSV / data export | `django-import-export` |
| Schema posture | Unmanaged models / `inspectdb` — Django reads the schema, does not own migrations |
| Mutations | Routed through gRPC to the Rust feature-store / compliance services so the same enforcement applies |
| Auth | Staff SSO (OIDC via `mozilla-django-oidc` or `django-allauth`); separate auth surface from customer JWTs |

### Infra

| Component | Technology |
|-----------|------------|
| Orchestration | Docker Compose dev → Kubernetes prod, Helm; see §11.5 for full topology |
| **K8s distribution** | **Cloud-managed (EKS / GKE / AKS — TBD at kickoff)** |
| **Node pools** | **`general-pool` (CPU, autoscaled) + `gpu-pool` (NVIDIA T4/L4/A10, taint-isolated)** |
| **Database operator** | **CloudNativePG (Postgres failover, PITR, backups)** |
| **Graph DB** | **Neo4j Helm chart + PVC** |
| **Cache / queue** | **Redis Operator (or Bitnami Helm)** |
| **Object storage** | **MinIO Operator** |
| **Workflow engine** | **Argo Workflows (training jobs + DB migrations)** |
| **Event-driven autoscaling** | **KEDA (queue-depth scaling for `feature-deriver`)** |
| **Ingress** | **Traefik + cert-manager + Let's Encrypt** |
| **Secrets** | **External Secrets Operator + cloud KMS / Vault** |
| **GitOps controller** | **ArgoCD (App-of-Apps + Image Updater)** |
| **Progressive delivery** | **Argo Rollouts (canary, metric-gated, auto-rollback)** |
| CI | GitHub Actions; `cargo nextest` + `sccache` for Rust; pytest for Python; `buf` for proto lint; cross-language integration test stage |
| **Image build** | **Multi-stage Dockerfiles → distroless base (`gcr.io/distroless/cc` Rust, `gcr.io/distroless/python3` Python); pushed to GHCR** |
| **Image scanning** | **Trivy (block on HIGH/CRITICAL)** |
| **SBOM** | **Syft on every release** |
| **Image signing** | **Cosign (Sigstore keyless)** |
| **Metrics** | **Prometheus + Grafana** |
| **Logs** | **Loki** |
| **Traces** | **Tempo (or Jaeger); OpenTelemetry SDKs both planes** |
| **Alerting** | **Alertmanager → PagerDuty / Opsgenie** |
| **DB migration tools** | **`sqlx-migrate` (Rust) + Alembic (Python) + `manage.py migrate` (Django, unmanaged-elsewhere)** |
| Observability | OpenTelemetry traces + Prometheus metrics + Loki logs across both planes |

### Design Principles

- Adapter-layered ingestion.
- Explainability mandatory.
- Auditability — model + rule + feature lineage all immutable + timestamped.
- Tenant isolation — RLS, per-tenant keys, tenant-scoped pgvector namespaces.
- No leakage — public-domain fine-tune corpus only; tenant data only enters shared training via federated learning + DP.
- GraphQL externally, gRPC across the Rust/Python boundary, REST for partner-API where industry standard.
- Feature-tier enforcement at the data-access boundary, with compile-time guarantees on the Rust side.
- Distributional, not point — every quantity that has uncertainty is modelled with a distribution.
- **Polyglot by design, not accident** — language choice per service is explicit; cross-plane traffic is intentional.

## 8. The Four Reasoning Layers

(structure unchanged from v2.3 — folded references to Rust services where they contribute)

### 8.1 Layer 1 — Probabilistic / ML (Python)

Gradient-boosted ensembles (XGB / LGB / CatBoost), monotonic + quantile boosting, hierarchical Bayesian (PyMC / NumPyro), multi-task learning shared backbone, Bayesian network, Bayesian decision network (pyAgrum), Random Forest, Legal-BERT / Gemma 4 LoRA embeddings, heterogeneous GNN (see §8.5), ARIMA / Prophet, Cox PH / AFT survival, two-stage settlement-history model, personality / ideology / topic / centrality features, causal inference (DoWhy / EconML / IV / RD / DiD), stacked meta-learner + Bayesian model averaging + Mixture of Experts, conformal prediction with per-stratum reliability, counterfactual explanations (DiCE / Alibi), anomaly detection + counterfactual explanations, time-decay weighting, drift detection.

**Rust-supported subroutines:** the **decision-arithmetic core** in Rust is called from the Python ML service via gRPC for EV/CVaR computation over distributions produced by the Python models — meaningful speedup on the hot serving path.

#### Psychological Methodology Features (v2.9)

A coherent set of psychologically-grounded features that complement the personality + ideology stack:

- **Moral Foundations Theory (MFT) features.** Six dimensions (care / fairness / loyalty / authority / sanctity / liberty) extracted from opinion text via the Moral Foundations Dictionary 2.0 + Gemma 4 LoRA refinement. Strong predictor in morally-laden cases (civil rights, religious-liberty, family-law, sentencing leniency). Slots into the gradient-boosted ensemble + hierarchical Bayesian models as judge-level features.
- **Cognitive bias profile per judge.** Anchoring vulnerability (damages clustering around plaintiff first-ask vs independent base rate), status-quo bias (procedural-status-change grant/deny ratio), hindsight bias (post-hoc evaluation pattern), in-group favoritism (rulings on litigants sharing demographic / professional background). Operationalized following Guthrie / Rachlinski / Wistrich methodology. Per-judge bias score becomes a feature in the ML layer + a sensitivity slider in the workspace UI.
- **HEXACO personality.** Six-factor (replaces Big Five non-destructively — Big Five mappings retained). Honesty-Humility specifically used in attorney-tactic prediction and witness-credibility scoring. Same fine-tuning corpus as the Big Five extractor; the LoRA `personality-en` adapter is retrained for the six-factor output.
- **Cognitive-style features.** Need for Cognition (NFC) and ambiguity tolerance from authored text. Predicts how a judge handles novel / complex cases vs precedent-bound; informs strategic counterfactuals ("lead with novel theory or stick with precedent").
- **GDMS (decision-making style)** for attorneys — rational / intuitive / dependent / avoidant / spontaneous — predicts negotiation patterns. Derived from brief + deposition style.
- **Cialdini persuasion-style classification.** Six-dimensional encoding of attorney rhetorical style (reciprocity / commitment / social proof / authority / liking / scarcity emphasis). Combined with judge cognitive-style for predicted persuasion-effectiveness (high-NFC judges resist social proof, defer to authority, etc.).
- **Career-trajectory personality drift.** TGN-modelled HEXACO + MFT + temperament evolution across judge career stages (early career, mid-career, post-elevation). The graph already supports temporal node features; this is a feature-engineering pass on top.
- **Stress / pressure response signature.** How decision quality, length, and predictability covary with caseload pressure derived from temporal opinion data already feeding the time-series model.

All features carry the same Tier-A/B/D labels and feature-store metadata as the rest of the personality stack — feature-tier enforcement applies identically.

#### Quantum / Quantum-Inspired Methods (v2.10)

Simulated on classical hardware — PennyLane / Qiskit Aer / NVIDIA cuQuantum / quimb / tntorch — no real QPU dependency. Realistic value: ensemble diversity, different inductive biases, R&D credibility, quantum-ready for when hardware matures. *Not* raw speedup — simulated quantum is generally slower than equivalent classical on classical hardware.

- **Tensor networks (quimb + tntorch).** Quantum-inspired classical algorithms; run efficiently on regular CPU/GPU. Two specific applications: (a) **BDN inference scaling** — tensor-train decomposition keeps influence-diagram inference tractable when the graph grows large (complex civil-rights employment cases especially); (b) **high-order feature-interaction compression** in the gradient-boosted ensemble — tensor compression of feature crosses that would otherwise be combinatorially expensive. Genuine win, not "quantum-on-simulator" — runs efficiently on classical hardware.
- **Quantum kernel methods** as ensemble members. PennyLane computes quantum kernel matrices on small feature subsets (4–8 qubits, fits trivially on simulator), feeds classical SVM. Different feature-space geometry than RBF — encodes non-classical similarity structure. Contributes to ensemble diversity alongside XGBoost / LightGBM / CatBoost; not faster, but **different**.
- **QAOA on simulator** for three operational combinatorial problems: (a) discovery scheduling — which depositions to prioritize given budget + relevance; (b) expert-witness assignment — which expert × case × jurisdiction maximizes win probability under conflict-of-interest constraints; (c) motion prioritization — sequencing motions to maximize cumulative impact. ≤30 qubits → fits classical simulator. Runs alongside (not replacing) classical MILP solutions; produces solutions with different local-optima structure.
- **Quantum walks on the citation graph** as alternative to PageRank / betweenness centrality. Quantum walks have ballistic spreading vs classical diffusive spreading — surface different node-importance rankings. Adds an additional centrality feature into Layer 1 alongside existing classical ones.
- **Variational Quantum Classifier (VQC)** as ensemble member. Small qubit count (4–12), classical-readout layer for binary case-outcome predictions. Contributes a different inductive bias to the ensemble; modest accuracy contribution, real R&D credibility.
- **Amplitude-estimation-inspired classical importance sampling** for the Monte Carlo trial simulation engine. Quantum amplitude-estimation does MC with O(1/ε) samples instead of classical O(1/ε²); on simulator this is *slower*, but the algorithm structure informs **classical** importance-sampling weights for our existing MC engine. Speeds up the *classical* MC engine by 2–5× on tail-distribution estimation tasks where standard sampling wastes effort. This is the "quantum-inspired classical" pattern again — runs efficiently on regular hardware.

The hybrid quantum-classical training loops (PennyLane + PyTorch) are structured so that swapping in a real QPU later is a config change, not a rewrite — quantum-ready architecture without the production hardware cost.

#### Generative & Latent-Variable Methods (NEW)

A focused sub-layer of generative models that fill specific gaps in the prediction stack. None replace the primary outcome models — each addresses a concrete failure mode the primary models can't.

- **β-VAE for tabular missing-data imputation.** Court records have meaningful missingness — partial attorney records, judges with thin career history, sparse expert-witness data. A β-VAE trained on full case feature vectors imputes missing features with **uncertainty estimates that propagate through the rest of the stack** (the conformal layer consumes the imputation variance). Better than mean / MICE imputation when missingness is non-random. Library: PyTorch β-VAE; `miceforest` retained as the simpler-but-strong baseline for ablation.
- **CEVAE for latent-confounder causal inference.** The existing causal layer (DoWhy + IV + propensity + RD + DiD) handles observed confounding. CEVAE jointly learns a latent-confounder representation, useful where IVs are weak (most state-court contexts). Slots into the causal sub-layer as an additional estimator alongside DoWhy / EconML.
- **Domain-adversarial training in the MTL trainer.** Adversarial domain-classifier loss across (Federal × CA × NJ) forces the shared MTL backbone to learn jurisdiction-invariant representations, beyond what hierarchical Bayes already provides. Library: `pytorch-domain-adaptation`. Implemented as an additional loss term, not a separate model.
- **TabDDPM (with DP) for synthetic-data sharing.** Tabular denoising-diffusion probabilistic model with differential privacy guarantees — generates synthetic case data that preserves statistical properties while protecting individual records. Used **only** for cross-tenant pre-training cold-start and academic-partnership data sharing; never as primary training data for the prediction model. Replaces the older DP-GAN approach with the more stable, mode-collapse-resistant diffusion equivalent. Coordinates with the federated-learning + DP framework — see §9.

Synthetic data is **only** used for federated-learning cold-start and academic data sharing, never for evaluation or as the primary training source. The risk that synthetic cases drift from real legal patterns is real; we mitigate by treating synthetic data as a pre-training augmentation only, with all final training and evaluation on real cases.

### 8.2 Layer 2 — Logic (Python)

Datalog / defeasible-logic rule engine; Z3 SMT (sentencing, means test, damages caps, UCC §2-718); argumentation frameworks (Dung AF, ASPIC+); description logic / OWL ontology (owlready2); temporal logic (LTL/CTL); deontic logic; Dempster–Shafer evidence aggregation; state-space / HMM; process mining (PM4Py).

### 8.3 Layer 3 — NLP + Fuzzy Logic (Python)

Document parsing → sentence/clause segmentation (spaCy) → legal NER → relation extraction → element classification (Gemma 4 + `judicialpredict-en` LoRA) → self-consistency / CoT verification → personality extraction (Gemma 4 + `personality-en` LoRA + LIWC) → topic modelling (BERTopic) → fuzzy element-membership scoring (`scikit-fuzzy`) → confidence-weighted output.

Active learning (modAL); tabular PDF extraction (Camelot / pdfplumber).

### 8.4 Layer 4 — Decision / Action (mixed)

**Python-side orchestration; Rust-side hot loops.**

- Risk-aware decision theory (EV + CVaR) — Python orchestrator calls Rust `decision-arithmetic core` for sampling-heavy inner loops.
- Distributional cost-of-litigation engine — **Rust service** consuming process-mining outputs and ML-derived component distributions.
- Game-theoretic settlement bargaining (`nashpy`) — Python; iteration counts fit in process so no Rust call needed.
- Stochastic dynamic programming (`mdptoolbox`) — Python.
- **Monte Carlo trial simulation engine — Rust service** (`rayon` + `ndarray`); 10-100× more trajectories per second than `simpy`.
- Bayesian decision networks (`pyAgrum`) — Python.
- Robust / worst-case optimisation (`cvxpy`) — Python.
- Judge–attorney compatibility scoring — Python (uses GNN embeddings).
- Lead-attorney optimization, expert-witness selection — Python orchestrator + Rust feature-store.
- Recommendation rule synthesis — Python with Rust-served EV/CVaR + Monte Carlo outputs.
- **Prospect-theory utility curves (v2.9)** — Kahneman–Tversky utility function with standard parameterization (α ≈ 0.88, β ≈ 0.88, λ ≈ 2.25); reference point per-case (status-quo zero vs expected-damages anchor) firm-configurable. Loss aversion + reference dependence are explicit, not implicit; feeds Nash bargaining + SDP utility evaluations.
- **Negotiation psychology (v2.9)** — anchor-and-adjust first-offer dynamics, framing-effect predictions on lead-with-strength vs lead-with-weakness, explicit BATNA / WATNA / ZOPA modelling. Augments (does not replace) the existing Nash + Rubinstein bargaining work; predicts realistic sequential-offer trajectories rather than single-shot equilibria.
- **Procedural justice multiplier (Tyler theory, v2.9)** — settlement-acceptance probability is multiplied by a procedural-justice score derived from process-mining outputs (motion-grant ratios per side, hearing-time per side, opinion-attentiveness signals). Settlements where the client felt heard are accepted at higher rates than economically-equivalent ones; the recommendation rule weights this in.

### 8.5 Heterogeneous Graph Neural Network (Python)

Architectures (HGT, R-GCN, TGN, GraphSAGE, GAT); KG embeddings (RotatE / ComplEx / TransE); tasks (outcome prediction, comparable-case retrieval, link prediction, judge/attorney embeddings, citation lean); network-analysis layer (PageRank, betweenness, eigenvector, Louvain/Leiden); training pipeline; sensitivity-tag enforcement (Tier-C node attributes excluded from message passing — enforced at the Rust feature-store boundary before the data ever reaches PyG).

## 9. Federated Learning & Differential Privacy

(unchanged in capability from v2.3 — Python-side coordinator running PySyft / Flower / TFF, DP-SGD via Opacus, secure aggregation, per-tenant DP budget; Rust-side feature-store enforces no Tier-C data ever enters the federation)

The federated-learning coordinator is Python (Flower is Python). The **secure-aggregation transport runs through the Rust gateway** — local model updates flow through a Rust streaming endpoint that encrypts, batches, and forwards to the Python coordinator. This separation means privacy-relevant traffic never touches Python TLS termination, and the gateway can rate-limit and audit per-tenant participation in a single place.

**DP-protected synthetic data sharing (v2.5).** TabDDPM-with-DP (see Layer 1, §8.1 *Generative & Latent-Variable Methods*) augments the federation with a second privacy-preserving sharing modality: a tenant can publish a DP-protected synthetic dataset distilled from their case corpus, which other tenants pre-train on. This addresses the federated-learning cold-start problem (a new tenant joining with too little data to contribute meaningful gradients). The synthetic-data pipeline runs on the same per-tenant DP budget as gradient sharing; the privacy accountant tracks both modalities under unified (ε, δ) accounting. Synthetic data is published into a tenant-isolated bucket; consumption by other tenants is logged and rate-limited. Synthetic data is **never** used as the primary training source for the prediction model — only as pre-training augmentation, with all final training and evaluation on real cases.

## 10. Quality, Testing & Robustness Discipline

- **Conformal coverage audit** (Python QA harness).
- **Property-based testing** — `hypothesis` for Python (rule engine, ML transforms), `proptest` for Rust (feature-store invariants, cost-engine invariants, simulation-engine invariants). Generates random fact patterns and asserts legal/economic invariants.
- **Adversarial robustness testing** — `textattack` (Python) for input adversarial examples; Rust-side fuzzing via `cargo fuzz` for the gateway and ingestion services.
- **Calibration audits.**
- **Compliance audit tests.**
- **Concept-drift tests.**
- **Cross-plane integration tests** — each `.proto` change triggers contract tests on both sides; `buf breaking` blocks incompatible changes; integration test environment spins up both planes with matched commits.
- **Standard automated test suite** — `pytest` + Playwright + `cargo nextest`; 70%+ coverage threshold per language.

## 11. Partner API & Integrations

**Surface served by the Rust gateway:**

- GraphQL endpoint for rich queries.
- REST endpoints for simpler resource operations.
- Webhooks for event-driven updates.

**Auth model:** OAuth 2 with scoped tokens; per-partner application registration; rate-limit + quota tracking.

**Initial integrations:** Clio (Clio Manage API), MyCase, NetDocuments, generic Zapier / Make.com bridge.

**Partner ecosystem:** public docs, Postman collections, sample integration code in TypeScript and Python.

## 11.5 Platform — Kubernetes + GitOps (NEW v2.8)

The platform layer is its own discipline, not a deployment target. Specified here so the application architecture above has a concrete operational counterpart.

### Cluster topology

- **Single shared multi-AZ Kubernetes cluster** (cloud provider TBD at kickoff — AWS EKS, GCP GKE, or Azure AKS; managed control plane in all cases).
- **Two node pools:**
  - `general-pool` — stateless services (gateways, inference, NLP, logic, decision orchestrator, Django admin, partner gateway, ingest-fetcher, feature-deriver). Standard CPU instances, autoscaled.
  - `gpu-pool` — `ml-training-job`, fine-tune workloads, embedding generation. NVIDIA T4 / L4 / A10. Cordoned via taints + tolerations so general workloads cannot land on GPU nodes.
- Future option: **namespace-per-tenant** for regulated tenants demanding hard isolation. Architecturally supported, not Phase 1 default.

### Workload patterns

- **Deployments + HorizontalPodAutoscaler** for stateless services.
- **StatefulSets + PVCs** for the data layer.
- **Operators** where they earn their keep:
  - **CloudNativePG** for Postgres — managed failover, point-in-time recovery, automated backups.
  - **Neo4j Helm chart** with PVCs for graph data (CE or AuraDB depending on commercial decision).
  - **Redis Operator** (or Bitnami Helm) for Redis 7 + Streams.
  - **MinIO Operator** for object storage.
- **Argo Workflows** for `ml-training-job` orchestration and database-migration jobs (more expressive than raw K8s Jobs for DAGs).
- **KEDA** for event-driven autoscaling — `feature-deriver` scales by Redis Streams queue depth, not request rate.

### Networking & ingress

- **Traefik** as ingress controller.
- **cert-manager** + Let's Encrypt for TLS.
- **NetworkPolicies** enforcing tenant isolation: customer traffic flows only main-gateway → ML inference; cross-namespace traffic explicitly allowlisted.
- **No service mesh in Phase 1.** Istio is too much operational overhead for ~22 services. Linkerd added later if mTLS-everywhere becomes a compliance requirement.

### Secrets management

- **External Secrets Operator** sourcing from cloud KMS (AWS Secrets Manager / GCP Secret Manager) or **Vault** if self-hosted.
- Never raw `Secret` resources committed to git. Sealed Secrets only as a fallback.

### GitOps pipeline

**Mono-repo (already chosen in v2.4) + `gitops/` subdirectory pulled by ArgoCD.**

```
mono-repo/
  rust/        — Rust workspace
  python/      — Python services + Django admin
  protos/      — gRPC contracts
  charts/      — Helm charts per service
  gitops/      — ArgoCD Application manifests + per-env values
  .github/     — CI workflows
```

**ArgoCD App-of-Apps:**

- One root `Application` per environment (`dev`, `staging`, `prod`) referencing per-service Applications.
- **Auto-sync** for `dev`; **manual sync** for `staging` and `prod` (`syncPolicy: manual`).
- **Promotion is a PR** to the `gitops/` directory bumping the image tag and chart version; CI runs preflight; reviewer approves; ArgoCD reconciles forward.
- **ArgoCD Image Updater** (or Renovate) keeps non-app dependencies fresh.

### CI pipeline (GitHub Actions)

1. **Lint + format** — Rust (`cargo fmt --check` + `cargo clippy -- -D warnings`), Python (`ruff` + `black --check`), proto (`buf lint` + `buf breaking`).
2. **Build** — `cargo build --release` with `sccache`; Python wheels; Django `collectstatic`.
3. **Test** — `cargo nextest`, `pytest -n auto`, Playwright e2e against compose dev stack, `proptest` + `hypothesis` property-based suites.
4. **Cross-plane integration** — spin up matched Rust + Python + Django images in CI, run gRPC contract tests.
5. **Container build** — multi-stage Dockerfiles → distroless base images (`gcr.io/distroless/cc` for Rust, `gcr.io/distroless/python3` for Python).
6. **Security gates** — **Trivy** image scan blocks on HIGH/CRITICAL; **Syft** generates SBOM; **Cosign** keyless signs via Sigstore.
7. **Push** — images to GHCR; Helm charts to OCI registry.
8. **GitOps update** — automated PR to `gitops/dev/` bumping image tag and chart version.

### Progressive delivery

- **Argo Rollouts** for `prod` deploys.
- Canary stepping: **5% → 25% → 50% → 100%**, each step gated by a **Prometheus metric query**:
  - P99 latency within tolerance.
  - Error rate within tolerance.
  - **Conformal-coverage drift** (model-quality SLO, not just infra metric) within tolerance.
  - gRPC contract-error rate within tolerance.
- **Auto-rollback** on metric regression.

### Database migrations

- **Decoupled from app deploys** to avoid lockstep coupling.
- **Argo Workflows preflight job** runs migrations *before* the corresponding app deploy, gated on success.
- **`sqlx-migrate`** for Rust services that own schema; **Alembic** for any Python service that needs to migrate; Django `manage.py migrate` for Django-owned tables only (unmanaged elsewhere).
- Breaking schema changes must merge with both the migration *and* consuming-service updates; CI rejects partial migrations.

### Observability

- **Prometheus + Grafana** for metrics (per-service exporters via the existing `metrics` crate / `prometheus_client`).
- **Loki** for logs (unified across both planes).
- **Tempo** (or Jaeger) for distributed traces; OpenTelemetry SDK already in v2.4 stack — both planes send to the same collector with correlation IDs threaded through every gRPC call.
- **Alertmanager → PagerDuty / Opsgenie** for paging.
- **Per-service SLOs** — P99 latency, error rate, conformal-coverage drift, gRPC contract-error rate, queue depth (for streamed services). SLO breaches drive Argo Rollouts auto-rollback and on-call alerts.

### Environment progression

| Env | Sync | Data | Purpose |
|-----|------|------|---------|
| `dev` | Auto | Synthetic / fake fixtures | Continuous integration; per-PR preview environments |
| `staging` | Manual via PR | Synthetic + opt-in pilot-firm test data | Pre-prod verification, federated-learning rehearsal, partner-API integration tests |
| `prod` | Manual via PR + approval | Real tenant data, full compliance posture | Customer-serving |

### Cost & operational footprint

- One **Senior SRE / Platform Engineer** is the structural team addition for v2.8 — owns the cluster, GitOps, observability, on-call rotation. Casey Muller continues as DevOps (CI, build pipelines, security audits, release engineering); SRE is a separate role.
- GPU-pool cost is the largest single line item — sized for nightly retrains + on-demand fine-tunes; spot instances where workload tolerates interruption.
- Multi-AZ for `prod` only; `dev` and `staging` single-AZ to control spend.

## 11.6 Engineering Methodology — Agile + XP + TDD + BDD + SOLID + DRY + Pragmatic FP (NEW v2.12)

Methodology is its own discipline; specified explicitly so practices are durable rather than incidental.

### 11.6.1 Agile cadence

- **Two-week sprints.** Tested at one-week and two-week cadences in pre-Phase-1; two weeks chosen as the right rhythm for cross-language work that crosses Rust + Python + Django + Next.js.
- **Ceremonies (per sprint):**
  - **Sprint Planning** (2h, Monday wk-1): backlog refinement, story commitment, definition-of-done check.
  - **Daily Standup** (15 min, every day): yesterday / today / blockers; not a status report.
  - **Mid-sprint Backlog Refinement** (1h, Wednesday wk-1): groom upcoming stories with PO + Dev + QA (Three Amigos format — see §11.6.4).
  - **Sprint Review / Demo** (1h, Friday wk-2): demo to PO + stakeholder; working software only, no slides.
  - **Sprint Retrospective** (1h, Friday wk-2): start / stop / continue; one concrete improvement committed each retro.
- **Roles:**
  - **Product Owner** — Alex Reeves (Operations Director); single source of priority truth.
  - **Scrum Master / Agile Coach** — Jamie Okafor (Project Manager); enforces ceremonies and methodology, not story authorship.
  - **Development Team** — cross-functional; everyone is a "developer" regardless of specialty (designer, ML, frontend, etc.).
- **Backlog tooling:** **Linear** (or Jira if firm preference dictates). Stories link to PRs; PRs link to stories; bidirectional.
- **User-story format:** "As a *(persona)*, I want *(capability)*, so that *(outcome)*." Acceptance criteria in **Gherkin Given/When/Then** (see §11.6.4 BDD).
- **Definition of Ready (DoR):** acceptance criteria written in Gherkin; persona identified; design / wireframe attached when UI-touching; gRPC contract clarified when cross-plane; no upstream blockers.
- **Definition of Done (DoD):** code merged to `main`; tests written and passing (TDD); BDD scenarios green; coverage above threshold; CI green (lint + format + build + Trivy + Syft + Cosign + a11y + Lighthouse); reviewed by 1+ engineer; documentation updated in same PR; ArgoCD has reconciled into `dev`; PostHog event schema updated when applicable; feature flag created when applicable.
- **Story pointing:** Fibonacci (1, 2, 3, 5, 8, 13). Stories larger than 8 split before commitment. Velocity tracked but never used as performance review — diagnostic only.
- **Sustainable pace:** 40-hour weeks the norm; on-call rotation paid time-off-in-lieu; no death marches. XP non-negotiable — see §11.6.2.

### 11.6.2 Extreme Programming (XP) practices

- **Pair programming** as the default for non-trivial work — design, ML modelling, gRPC contract authorship, security-sensitive code, compliance enforcement, rule-engine encoding. Solo work fine for routine implementation. Mob programming for hard cross-cutting problems (architecture decisions, incident response).
- **Trunk-based development.** Single long-lived `main` branch; short-lived feature branches (≤2 days); merged via PR with squash. No long-running release branches; release-train via GitOps environment progression (see §11.5).
- **Collective code ownership.** Anyone can edit any code. Git blame is for understanding, not blame.
- **Continuous integration.** Every commit triggers full CI (see §11.5 + §11.6.7); broken `main` is a stop-the-line event — first responsibility is to fix or revert, then resume normal work.
- **Sustainable pace.** No 60-hour weeks. Compounding fatigue produces compounding bugs in safety-sensitive code, which legal-prediction software absolutely is.
- **YAGNI (You Aren't Gonna Need It).** Implement only the user story in front of you; the spec lists Phase 2 features for a reason.
- **Simple design.** Easiest-thing-that-works is preferred; abstractions earn their place by the rule of three (see DRY §11.6.6).
- **Refactoring continuously.** Always-leave-it-cleaner-than-you-found-it. Refactoring is part of the sprint, not a separate sprint.
- **Coding standards:** enforced via `cargo fmt` + `cargo clippy -- -D warnings` (Rust); `ruff` + `black` (Python); `eslint` + `prettier` (TypeScript); `buf lint` (proto). No exceptions.
- **Customer on-site (or close substitute):** PO available daily; pilot-firm rotating point-of-contact for usability rounds.

### 11.6.3 Test-Driven Development (TDD)

- **Red → Green → Refactor** as the default development cycle.
- **Test pyramid:**
  - **Unit tests** (most) — pure-function tests, fast, isolated. `pytest` (Python), `cargo nextest` (Rust), `vitest` (TypeScript), `pytest-django` (Django).
  - **Integration tests** (fewer) — service boundaries, gRPC contracts, DB interactions. Spin up test containers; use `testcontainers-rs` and `testcontainers-python`.
  - **End-to-end tests** (fewest) — Playwright across the customer-facing app; cross-plane integration tests for full case-evaluation flow.
- **Coverage thresholds (per-language, CI-enforced):** 70% line + branch + function + statement minimum; >85% for compliance-enforcement code (Rust feature-store) and rule-engine code (Python logic service); 100% on the rule-engine invariants checked via property-based tests.
- **Property-based tests** (already in spec — `hypothesis` + `proptest`) for invariant-shape testing.
- **Mutation testing** (NEW v2.12) — `mutmut` (Python) + `cargo-mutants` (Rust) run weekly; mutation-survival > 5% triggers a backlog item to strengthen tests.
- **No test, no merge.** PRs without tests are returned in review.

### 11.6.4 Behavior-Driven Development (BDD)

- **Gherkin Given/When/Then** for every user story's acceptance criteria. The story is the test; the test is the story.
- **Three Amigos** sessions in mid-sprint refinement: Product Owner + Developer + QA jointly write the Gherkin before coding starts. No story enters Sprint Planning without Three-Amigos sign-off.
- **Tooling per language:**
  - **`pytest-bdd`** (Python) — preferred for ML/Logic/NLP services.
  - **`behave`** (Python) — for Django admin app.
  - **`cucumber-rs`** (Rust) — for Rust data plane.
  - **Cucumber.js** — for Next.js workspace + Django frontend integration.
- **Living documentation** — Gherkin features in `features/` directories double as the executable spec; automated extraction publishes them to the docs site (Docusaurus) so PMs and SMEs can read them as plain English.

### 11.6.5 SOLID principles

Applied with judgment — not dogma — and adapted per language:

- **Single Responsibility (SRP).** Every Rust crate, Python module, Django app, Next.js component does one thing. Compliance-enforcement is a dedicated crate; it never grows to also handle ingestion. Module-size guideline: ~700 LOC; split when clarity falls.
- **Open/Closed (OCP).** Code open for extension, closed for modification. Implemented via traits in Rust (`Pluggable<T>` for adapter layers — paid data sources, partner-API integrations, channel adapters), abstract base classes / Protocols in Python (the rule engine accepts new statutes without changing the engine), shadcn/ui composition patterns in the frontend.
- **Liskov Substitution (LSP).** Subtypes substitute base types without surprise. Specifically enforced for: Datalog rule subtypes; provider-adapter subtypes; ML-model subtypes (a champion → challenger swap must satisfy the same interface contract); fuzzy-MF subtypes.
- **Interface Segregation (ISP).** Many small, focused interfaces over one fat one. The Rust feature-store exposes `ReadAccess`, `WriteAccess`, `LineageAccess`, `AuditAccess` as separate traits — services consume only what they need. Python services define narrow Protocol classes for the same reason.
- **Dependency Inversion (DIP).** Depend on abstractions, not concretions. Rust uses traits + generic constraints; Python uses Protocol + dependency injection (constructor-based, no service locator). Tests benefit directly — the rule engine is testable against a fake feature-store, the ML inference service is testable against a fake graph service.

### 11.6.6 Don't Repeat Yourself (DRY) — with discipline

- **Single source of truth** for any piece of knowledge: a domain rule, a feature definition, a UI string, a gRPC schema. The `protos/` directory is the *only* place a contract is defined; the same `.proto` produces both Rust and Python clients. The `features/` Gherkin directory is the *only* place an acceptance criterion is written.
- **Rule of three before abstraction.** Three similar lines is better than a premature abstraction. Two duplicates is fine; the third triggers the refactor. Abstractions extracted prematurely cost more than they save — see §11.6.2 YAGNI.
- **DRY ≠ no duplication ever.** Some duplication is honest — two case-types with similar but not identical logic should be two clear functions, not one over-parameterized one. Codebase guideline: prefer two clear three-line functions over one four-parameter abstraction that handles both.
- **Cross-cutting deduplication:**
  - One source for design tokens (Figma + TS) — never re-typed in CSS.
  - One source for legal rules (Datalog corpus + version history) — never re-encoded in ML or NLP layers.
  - One source for compliance policy (feature-store metadata) — enforced everywhere via the type system.
  - One voice & tone guide — UI copy never re-written ad hoc.
  - One BDD scenario per behavior — never duplicated in unit tests; unit tests cover the *implementation* of behaviors that BDD specifies.

### 11.6.7 Pragmatic Functional Programming (v2.12, made concrete in v2.13)

The architecture is not pure-FP, but specific services are designated functional-core. The rule: **functional core, imperative shell.** Pure functions wherever they fit; effects pushed to the boundaries. This subsection enumerates the concrete commitments.

#### Designated functional-core (pure functions, no global state, referentially transparent, property-testable as algebraic invariants)

- **Rust `decision-arith` crate** — EV / CVaR / Nash / Rubinstein / Kalai-Smorodinsky / prospect-theory utility / procedural-justice multiplier are pure functions over distributions. Zero global state. `rayon` parallelism over the same pure functions. Property tests assert algebraic invariants (monotonicity, scale invariance where it should hold, units consistency, conservation properties).
- **Rust `monte-carlo-sim` crate** — trajectory generation is a pure closure `(seed, params) -> Trajectory`; the engine is a parallel `rayon::par_iter` over N seeds. Imperative shell only at the I/O boundary (loading params, persisting aggregates).
- **Rust `cost-engine` crate** — distributional cost composition is pure: `(component-distributions, correlation-matrix) -> total-distribution`. Statrs distributions are immutable values.
- **Rust `feature-store` types crate** — `Tier`, `Sensitivity`, `PermittedUse` newtype wrappers + ADTs + exhaustive `match` give compile-time tier enforcement. The *types are functional*; the storage layer (sqlx + Postgres) is the imperative shell.
- **Python `logic-service` rule-application functions** — given a fact base (immutable `frozendict` / Pyrsistent `pmap`) and a rule set, return derived facts. Pure. The Z3 calls are encapsulated as `(constraints) -> (model | unsat)` pure-function wrappers, even though the underlying solver is imperative.
- **Python `causal-inference` estimators** — given features + treatment + outcome, return ATE estimate + CI. Pure. DoWhy, EconML, CEVAE all wrapped as pure functions.
- **Python fuzzy-MF library** — `(facts, mf-spec) -> membership-score`. Pure.
- **Python conformal-prediction** (MAPIE) wrapped as `(model, calibration-set, x) -> prediction-interval`. Pure.

#### Functional-leaning idioms (mostly functional, with state where honest)

- **Rust API gateway** — request handlers are mostly pure transformations of `Request -> Response`; tokio's async runtime is the imperative shell. State (authentication, rate-limit counters) lives in well-isolated services injected as dependencies.
- **Python ML inference service** — model serving is `(features) -> prediction`, pure modulo the loaded model weights. Multi-task heads compose as function composition.
- **Next.js workspace state** — Zustand (or Redux Toolkit) pure reducers; no class components; hooks-only; immutable updates with Immer when ergonomics demand. Time-travel debugging works because reducers are pure.
- **ML data pipelines** — Polars LazyFrame for immutable transformation chains; never `df.column = ...` mutations. Generators for streaming where appropriate.
- **GraphQL gateway resolvers** — pure where data is cached or read-only; side-effect-isolated otherwise. DataLoader composes purely.
- **Property-based test generators** are pure by definition; lean on `hypothesis` / `proptest` extensively.

#### Imperative / object-oriented (state is the point — don't fight it)

- **Federated-learning coordinator** — round counters, privacy budgets, tenant participation history are intrinsically stateful. Object-oriented or actor-style is honest.
- **Training jobs** — model weights evolve; epoch state is real. PyTorch training loops stay imperative.
- **Model registry** (MLflow) — stateful by design.
- **Database / ORM layer** (Django ORM, SQLAlchemy, sqlx) — relational state is the point.
- **Ingestion fetchers** — I/O orchestration with checkpointing.
- **Real-time event broker** — connection-state management.
- **Defeater bookkeeping in the rule engine** — order of rule application matters; mutable working memory is honest about what's actually happening (the *individual rules* remain pure functions; the *engine* that schedules them is imperative).

#### Boundary rules

- **Effects at the edge.** Network calls, DB writes, file I/O, time, randomness — all explicit at service boundaries, never buried in nominally pure logic.
- **Pure cores composed via simple data structures.** Cross-module integration uses plain data (records, sum types, immutable collections), never shared mutable state.
- **No monad-transformer towers.** This is Rust + Python + TypeScript, not Haskell. Pragmatic FP, not category theory.
- **Don't fight the language.** Rust naturally rewards FP idioms; Python tolerates them; TypeScript is happy with hooks + reducers. Stay in the idiomatic-FP zone for each language; never write Haskell-in-Python.
- **Architectural Decision Record:** ADR-FP-001 documents these choices with rationale; future paradigm changes require an updated ADR.

#### What this buys us concretely

- **Property-based testing is dramatically easier** on pure functions — Hypothesis / proptest find counterexamples to algebraic invariants the unit tests miss. The decision-arith and feature-store crates are heavily property-tested for this reason.
- **Parallelism is free** in the functional-core services — rayon's `par_iter` works because there's no shared mutable state to coordinate. The Monte Carlo engine's 10–100× speedup over simpy is partly Rust, partly the trivially-parallel pure-function shape.
- **Compliance enforcement is compile-time, not runtime.** Rust's type system rejects Tier-C-flow violations because the `Tier::C` ADT variant cannot satisfy a `PermittedUse` bound that excludes it. Pure types over mutable state.
- **Replay and audit are honest.** Pure functions take their inputs explicitly; given the same inputs, they produce the same outputs. This is the core of reproducible legal analysis — a recommendation can be re-derived from the recorded inputs years later.
- **Reasoning chains compose.** Layer 2's argumentation-framework defeats compose with Layer 1's conformal intervals through pure-function plumbing; the imperative state lives only at the I/O edges.

#### What it costs

- **Rust learning curve for engineers used to OOP.** Mitigated by pair programming + ADR-FP-001 onboarding doc.
- **Some Python developers will want to write classes everywhere.** Code review enforces the boundary; the rule is "use a class when state is genuine, otherwise use a function."
- **Debugging pure deeply-nested functional pipelines requires different tools.** Use Polars' `.collect_schema()` / lazy plans for pipeline introspection; Rust's `tracing` crate with `instrument` spans for functional-core observability.
- **Pure-function discipline can produce verbose code** in Python compared to mutable shortcuts. Accept the verbosity; correctness on legal-prediction code matters more than line count.

### 11.6.8 DevOps Culture

Beyond the tooling already in §11.5:

- **DORA metrics** tracked from week 1: deployment frequency, lead time for changes, change failure rate, MTTR. Quarterly review against industry-elite benchmarks. Diagnostic only — never used in performance reviews.
- **SLOs / SLIs / error budgets.** Per-service SLOs published in Grafana; SLI definitions in code (alongside Prometheus exporters). When a service burns through its monthly error budget, feature work pauses until reliability work catches up — no exceptions.
- **Incident response.**
  - **PagerDuty** (or Opsgenie) on-call rotation across SRE + on-call dev representative from each plane.
  - **Severity matrix** documented (SEV1 = customer-impacting outage; SEV2 = degraded; SEV3 = bug).
  - **Incident commander** role rotates; declared on every SEV1+ incident.
  - **Status page** (statuspage.io or self-hosted) for tenant-facing transparency.
- **Blameless postmortems** within 5 business days of any SEV1+ incident. Format: timeline + impact + root cause + contributing factors + corrective actions + lessons learned. Postmortems are public within the team; the corrective-action list is tracked in the backlog.
- **Chaos engineering** starting in week 16 once `staging` is up. **LitmusChaos** running weekly chaos experiments in `staging`; quarterly **game days** rehearsing region failure, dependency failure, security incident, runaway ML inference, partner-API abuse. Each game day has a hypothesis, a chaos experiment, observed outcomes, and a postmortem-style writeup.
- **Feature flags as a deployment strategy.** Every customer-facing change ships behind a flag; flags rolled out 1% → 10% → 50% → 100% gated on metrics. Flags have explicit lifetimes (stale-flag dashboard surfaces flags older than 60 days).
- **Documentation as code.** Docs live in the repo, written in Markdown, reviewed in PRs. **Docusaurus** publishes the user-facing docs site; Gherkin-extracted living-doc pages publish automatically; the spec doc itself versioned in git.
- **Postmortem template + incident-response runbook** in `runbooks/` directory; updated as part of each incident's corrective actions.

### 11.6.9 Code Review Standards

- **Every PR reviewed by ≥1 engineer.** Compliance-touching changes require ≥2 reviewers including one from the Compliance Engineer's purview. Rule-engine changes require Legal SME sign-off.
- **Review SLA:** 4 working hours for normal PRs, 30 minutes for SEV-1-related PRs.
- **PR template** (in repo): linked story, acceptance criteria checklist, test plan, screenshots / Storybook diff for UI changes, performance impact, security impact, compliance impact.
- **Conventional commits** (`feat:` / `fix:` / `chore:` / `refactor:` / `docs:` / `test:`) — drives changelog generation.
- **Pre-merge checklist** in CI: format + lint + tests + coverage + property tests + a11y + Lighthouse + Trivy + Cosign + cross-plane integration. All green or no merge.

### 11.6.10 Tools

| Component | Technology |
|-----------|------------|
| Sprint management | **Linear** (or Jira) |
| Living documentation | **Docusaurus** (extracts Gherkin features) |
| Mutation testing | **mutmut** (Python) + **cargo-mutants** (Rust) |
| BDD | **pytest-bdd** + **behave** + **cucumber-rs** + **Cucumber.js** |
| Chaos engineering | **LitmusChaos** |
| Incident management | **PagerDuty** (or Opsgenie) + **statuspage.io** (or self-hosted) |
| DORA metrics | **Four Keys** (Google's open-source tool) or self-hosted Grafana dashboard |
| Postmortems | Markdown templates in `postmortems/` directory |
| Conventional commits | **commitlint** + automated changelog generation |
| Documentation | **Docusaurus** at `/docs` route; deployed via same GitOps pipeline |



(unchanged from v2.3 — case intake, workspace with summary card / factor breakdown / counterfactual / rule trace / comparables / attorney-judge graph / time-to-resolution / sensitivity / SDP / bargaining / Monte Carlo / cost breakdown / lead-attorney / expert-witness / network-position / topic-membership / compliance disclosure; firm admin; platform admin)

The **case workspace's WebSocket live-update channel runs through the Rust real-time event broker** — tokio-managed connections give consistent low-latency fan-out as case events arrive.

## 12.5 UI/UX Design Discipline (NEW v2.11)

UI/UX is a discipline, not a frontend stack. Promoted to first-class status in v2.11 because the v2.10 sketch ("Next.js + shadcn/ui + 16 panels") would land as 16 disconnected products glued together without explicit design work. Companion document: `judicialpredict-wireframes.md`.

### 12.5.1 Design system + component library

- **Source of truth:** Figma. All screens, components, tokens live there. Versioned via Figma branches.
- **Component implementation:** customised shadcn/ui + dedicated design tokens (color, spacing, typography, motion, elevation, shadow, radius). Tokens defined in TypeScript and consumed by both Tailwind (via theme extension) and any non-Tailwind surfaces.
- **Storybook** as the developer-facing component catalogue with controls, accessibility annotations, and interaction tests.
- **Chromatic** for visual-regression testing on every PR. CI gate blocks merges that introduce unintended visual changes.
- **Design-token CI** — token changes are pull-requested and reviewed; updates propagate to Tailwind config, Figma library, and Storybook simultaneously.

### 12.5.2 User research + personas

Five distinct personas drive design decisions:

- **Partner** — decision authority, time-poor, glances Summary tab and walks. Default view optimised for them.
- **Associate** — does the case work, lives in Outcome / Strategy / Bargaining tabs.
- **Paralegal** — intake + document management, needs efficient data entry + clear progress feedback.
- **Ops / Litigation Support** — firm admin, billing, partner-API integrations.
- **Compliance Officer** — audit log, disparate-impact reports, feature-tier policy.

Method: 12–15 contextual interviews at pilot firms in weeks 1–4; ongoing usability testing rounds at weeks 22, 28, 33; persona docs maintained as living artifacts in the design repo.

### 12.5.3 Information architecture

Sixteen+ workspace panels = navigation problem. IA decisions:

- **Default view = partner-facing Summary** with the recommendation, EV, CVaR, and one-paragraph reasoning.
- **Tabbed workspace** (Summary / Outcome / Strategy / Bargaining / Comparables / Timeline / Compliance / Memo) — never 16 sibling panels.
- **Per-user persisted state** — associate's expanded view differs from partner's.
- **Progressive disclosure** — every advanced view has a "Show more" path from the Summary; partners never *have* to click into them.

Wireframes: see companion document, sections 5–11.

### 12.5.4 Data visualization design language

Every panel is a viz component; without a consistent grammar the workspace fragments. Decisions:

- **Uncertainty visualization standard** — gradient bands for conformal CIs, fan charts for MC distributions, error bars consistent across all panels.
- **Color-blind-safe palette** (Okabe–Ito 8-color) — every recommendation state distinguishable in greyscale.
- **No information conveyed by colour alone** — pair with shape / pattern / text.
- **Shared tooltip schema** across all panels.
- **Standard axis conventions** — time on X, magnitude on Y, ranges always with quantile annotations.
- **Charts have text-equivalent representations** — toggle to data-table view for screen readers and export.

### 12.5.5 Accessibility — WCAG 2.2 AA from day one

Non-negotiable. Enterprise law-firm procurement reviews block products that don't meet AA.

- **CI gates:** axe-core on every PR; Pa11y monthly full-site audit; Lighthouse accessibility score floor.
- **Manual testing:** NVDA + VoiceOver passes on every release. Keyboard-only navigation pass.
- **Compliance audits:** quarterly external accessibility consultant review.
- See companion document §16 for the full WCAG 2.2 AA checklist.

### 12.5.6 Print / PDF memo design

The PDF case-evaluation memo is the artifact partners share with clients — design quality directly affects perceived credibility.

- Server-side rendering via **Puppeteer** (or **WeasyPrint** if license terms favor it) over a dedicated print stylesheet route.
- Typography, layout, chart rendering at 300 dpi, page-break logic, table of contents, signature block, methodology notes section, compliance statement.
- The memo should look like Cravath sent it — not a Streamlit dashboard print-screen.
- Wireframe: companion document §11.

### 12.5.7 Onboarding flows

Three distinct paths:

- **Firm onboarding** — admin sets up tenant, configures feature-tier policy, invites users, optionally opts into federated learning.
- **User onboarding** — role-based first-run, guided first-case walkthrough, "training mode" with synthetic data so the user can practice without consequences.
- **Integration onboarding** — Clio / MyCase / NetDocuments hookup wizard.

Abandoned-onboarding is the most common B2B-tool failure mode; each path needs explicit design + analytics instrumentation.

### 12.5.8 State catalogue

Every panel must implement: Default, Loading, Empty, Partial, Error, Stale, Permission-denied, Tier-blocked. See companion document §15 for patterns.

### 12.5.9 Performance budgets

Enforced in CI via Lighthouse CI:

| Metric | Budget |
|--------|--------|
| LCP | < 2.0s P95 |
| INP | < 200ms P95 |
| CLS | < 0.1 |
| Initial JS | < 250 KB compressed |
| Workspace cold load | < 4s P95 |
| Recommendation refresh after slider | < 600ms P95 |

Lighthouse CI runs on every PR; budget regressions block merge. RUM via PostHog catches what synthetic CI can't.

### 12.5.10 Product analytics + feature flags

- **PostHog** (self-hosted for compliance) — funnel analysis, cohort retention, session replay (with PII masking enforced at the SDK level).
- **GrowthBook** (or Unleash) — feature flags for gradual rollouts, A/B-ready scaffolding (actual A/B testing deferred to Phase 2).
- Per-event schema reviewed in design + product before instrumentation lands; analytics is a designed surface, not afterthought tracking.

### 12.5.11 Brand identity

Logo, color palette beyond functional tokens, typography hierarchy, voice/tone guide. Without explicit brand work the product reads as generic; with brand work it has presence. Brand work runs weeks 3–10 in parallel with design system; brand consultancy contracted or internal designer-led depending on hire.

### 12.5.12 Tablet responsive design

Lawyers use iPads in deposition rooms and at counsel table. Workspace usable at iPad-Pro-landscape resolution. Single-column collapse, modal sensitivity sliders, simplified graph visualizations. Not native iOS — responsive web. Mobile (<768 px) explicitly out of scope Phase 1.

### 12.5.13 Internationalization scaffolding

Phase 1 is English-only, but i18n is much harder to retrofit than scaffold. **next-intl** in place from day one; all UI strings in message catalogues; locale-aware date/number/currency formatting; only `en-US` shipped.

### 12.5.14 Microinteractions / motion design

Panel transitions, recommendation-flip animations, sensitivity-slider feedback, success/error toasts. **Framer Motion**. Polish, not foundation — but motion design is where "enterprise tool" becomes "modern product." Respects `prefers-reduced-motion`.

### 12.5.15 Voice and tone guide

UI copy is product. Documented in `judicialpredict-wireframes.md` §18. Principles: direct not hedged, quantified not vague, calibrated not boastful, no anthropomorphism of the AI ("the model predicts" not "JudicialPredict thinks"), numbers always with units and reference points, plain English over jargon for partner-facing surfaces.

### 12.5.16 Research-led design cadence

UX research and design system work *lead* frontend implementation. Sequence:

- **Wk 1–4:** UX research, persona development, IA design.
- **Wk 3–8:** Design-system scaffold, component library, brand work in parallel with research.
- **Wk 6 onward:** Frontend implementation accelerates against the design system.
- **Wk 14 onward:** Print/PDF memo design + onboarding flow design.
- **Wk 22, 28, 33:** Usability testing rounds with pilot firms; iterate.

This shifts frontend implementation milestones forward by ~3 weeks; total Phase 1 timeline lands at ~41 weeks (was 38).

## 13. Multi-Tenancy

(unchanged from v2.3 — RLS, per-tenant keys, tenant-scoped pgvector namespaces, federated-learning opt-in, per-tenant feature-tier configuration, SOC 2 readiness)

## 14. Civil vs Criminal Handling

(unchanged from v2.3)

## 15. Delivery Milestones & Timeline (38 Weeks)

| # | Milestone | Plane | Start | End | Owner |
|---|-----------|-------|-------|-----|-------|
| 1 | Infra + CI (Rust + Python pipelines) + multi-tenant Postgres + Neo4j + Redis | both | Wk 1 | Wk 3 | DevOps |
| 2 | gRPC `.proto` contracts + codegen pipeline + buf lint + breaking-change CI gate | both | Wk 1 | Wk 3 | Backend |
| 3 | **Rust API gateway scaffold** — axum + async-graphql + JWT + RBAC + tenant + feature-tier middleware | Rust | Wk 2 | Wk 6 | Rust Eng |
| 4 | **Rust feature store + compliance enforcement** — sqlx + tier/sensitivity types | Rust | Wk 3 | Wk 8 | Rust Eng + Compliance |
| 5 | **Rust ingestion pipelines** — CourtListener, CAP, Cornell LII, CA, NJ | Rust | Wk 3 | Wk 10 | Rust Eng |
| 6 | Ideology + demographic ingestion: MQ, JCS, Bonica DIME, FEC, FJC bios | Rust + Python | Wk 4 | Wk 10 | Rust Eng / Backend |
| 7 | Tabular PDF extraction pipeline (Camelot / pdfplumber) | Python | Wk 4 | Wk 7 | NLP |
| 8 | Knowledge-graph schema + node/edge ingestion + Neo4j load | Rust + Python | Wk 5 | Wk 10 | Rust Eng / Graph |
| 9 | Network analytics: centrality + community detection (NetworkX/cuGraph) | Python | Wk 8 | Wk 11 | Graph |
| 10 | NLP pipeline v1: spaCy + legal NER + element extraction | Python | Wk 6 | Wk 10 | ML |
| 11 | Self-consistency / CoT verification layer | Python | Wk 9 | Wk 11 | NLP |
| 12 | Topic modelling (BERTopic / Tomotopy) | Python | Wk 9 | Wk 12 | NLP |
| 13 | Gemma 4 LoRA fine-tune (judicialpredict-en) | Python | Wk 8 | Wk 12 | ML |
| 14 | Personality extraction service + LoRA (personality-en) + LIWC | Python | Wk 9 | Wk 14 | NLP / Personality |
| 15 | Fuzzy logic layer + 7 MFs + active learning | Python | Wk 9 | Wk 12 | ML |
| 16 | Hierarchical Bayesian models (PyMC / NumPyro) | Python | Wk 10 | Wk 14 | ML |
| 17 | Gradient-boosted ensemble + monotonic + quantile | Python | Wk 10 | Wk 14 | ML |
| 18 | Multi-task learning architecture | Python | Wk 12 | Wk 15 | ML |
| 19 | Two-stage settlement-history model | Python | Wk 13 | Wk 16 | ML |
| 20 | Time-decay weighting | Python | Wk 13 | Wk 14 | ML |
| 21 | Heterogeneous GNN (HGT + R-GCN + TGN + GraphSAGE) | Python | Wk 11 | Wk 18 | Graph |
| 22 | KG embeddings (RotatE / ComplEx) + link prediction | Python | Wk 13 | Wk 17 | Graph |
| 23 | Causal inference layer (DoWhy / EconML / IV) | Python | Wk 12 | Wk 17 | Causal |
| 24 | Conformal prediction (MAPIE) + per-stratum reliability | Python | Wk 14 | Wk 17 | ML |
| 25 | Survival models (lifelines: Cox PH, AFT) | Python | Wk 14 | Wk 17 | ML |
| 26 | Bayesian decision network (pyAgrum) | Python | Wk 15 | Wk 18 | ML |
| 27 | Mixture-of-experts gating + Bayesian model averaging | Python | Wk 15 | Wk 18 | ML |
| 28 | Anomaly detection + counterfactual anomaly explanations | Python | Wk 16 | Wk 18 | ML |
| 29 | Counterfactual explanations (DiCE / Alibi) | Python | Wk 16 | Wk 19 | ML |
| 30 | Concept-drift detection (River / Alibi Detect / Evidently) | Python | Wk 15 | Wk 18 | ML |
| 30a | **β-VAE missing-data imputation (PyTorch)** | Python | Wk 14 | Wk 17 | ML |
| 30b | **CEVAE for latent-confounder causal inference** | Python | Wk 15 | Wk 18 | Causal |
| 30c | **Domain-adversarial loss in MTL trainer (cross-jurisdiction)** | Python | Wk 15 | Wk 17 | ML |
| 30d | **TabDDPM-with-DP synthetic-data pipeline + DP accountant integration** | Python | Wk 22 | Wk 26 | Federated / ML |
| 30e | **Django admin app scaffold + tenant + user + RBAC models (unmanaged)** | Django | Wk 8 | Wk 12 | Django Eng |
| 30f | **Django: rule-corpus editor + argumentation-framework editor + version history** | Django | Wk 14 | Wk 20 | Django Eng + Logic |
| 30g | **Django: audit log + lineage explorer + feature-store metadata browser + proxy-audit dashboard** | Django | Wk 18 | Wk 24 | Django Eng + Compliance |
| 30h | **Django: federated-learning coordinator dashboard + disparate-impact reports + partner-API token management** | Django | Wk 22 | Wk 28 | Django Eng + Federated + Compliance |
| 30i | **Django staff SSO (OIDC) + per-screen permission policies** | Django | Wk 10 | Wk 13 | Django Eng + DevOps |
| 30j | **Service split: `ml-inference-svc` extracted from ML service; horizontal scaling profile + p99 SLO** | Python | Wk 13 | Wk 16 | ML + DevOps |
| 30k | **Service split: `ml-training-job` on Airflow/Argo + GPU pool; nightly retrains decoupled from inference** | Python | Wk 14 | Wk 17 | ML + DevOps |
| 30l | **Service split: `llm-client-svc` (httpx pool + retry + rate-limit + circuit breaker + prompt cache)** | Python | Wk 9 | Wk 12 | NLP + DevOps |
| 30m | **Service split: `ingest-fetcher` and `feature-deriver` separated; Redis Streams handoff; replay tooling** | Rust | Wk 7 | Wk 11 | Rust Eng |
| 30n | **Service split: `partner-gateway` extracted from main gateway; OAuth 2 + per-partner rate-limit + abuse monitoring** | Rust | Wk 24 | Wk 28 | Rust Eng + Partner Eng |
| 30o | **K8s cluster bootstrap + node pools + namespaces + RBAC + NetworkPolicies** | Platform | Wk 1 | Wk 4 | SRE |
| 30p | **Stateful operators: CloudNativePG + Neo4j + Redis + MinIO** | Platform | Wk 2 | Wk 5 | SRE |
| 30q | **Ingress (Traefik) + cert-manager + External Secrets Operator + Vault/KMS** | Platform | Wk 3 | Wk 6 | SRE |
| 30r | **GitOps: ArgoCD + App-of-Apps + per-env values + Image Updater** | Platform | Wk 4 | Wk 7 | SRE |
| 30s | **CI: lint + build + test + Trivy + Syft + Cosign + GHCR push + gitops PR auto-bump** | Platform | Wk 3 | Wk 8 | SRE + DevOps |
| 30t | **Argo Workflows + Argo Rollouts + Prometheus-metric-gated canary + auto-rollback** | Platform | Wk 6 | Wk 10 | SRE |
| 30u | **Observability stack: Prometheus + Grafana + Loki + Tempo + Alertmanager + per-service SLO dashboards** | Platform | Wk 5 | Wk 12 | SRE |
| 30v | **DB migration scaffolding: sqlx-migrate + Alembic + Django migrations as Argo Workflows preflight** | Platform | Wk 7 | Wk 11 | SRE + Backend |
| 30w | **GPU-pool sizing + spot-instance policy + nightly-retrain Argo Workflows + cost dashboards** | Platform | Wk 10 | Wk 14 | SRE + ML |
| 30x | **UX research: 12-15 contextual interviews + persona development** | Design | Wk 1 | Wk 4 | UX Researcher |
| 30y | **Information architecture: workspace IA + navigation + state catalogue** | Design | Wk 2 | Wk 5 | Product Designer |
| 30z | **Design system v1: tokens + customised shadcn/ui + Figma library + Storybook** | Design + FE | Wk 3 | Wk 8 | Product Designer + FE Eng |
| 30aa | **Brand identity: logo + palette + typography hierarchy + voice/tone guide** | Design | Wk 3 | Wk 10 | Product Designer |
| 30ab | **Data visualization design language + uncertainty viz standards** | Design | Wk 5 | Wk 9 | Product Designer |
| 30ac | **Wireframes → high-fidelity mockups for all major flows** | Design | Wk 4 | Wk 12 | Product Designer |
| 30ad | **Accessibility scaffolding: axe-core CI + Pa11y schedule + WCAG checklist** | FE + A11y | Wk 6 | Wk 10 | A11y Consultant + FE Eng |
| 30ae | **Performance budgets + Lighthouse CI gate + RUM via PostHog** | FE + Platform | Wk 8 | Wk 12 | FE Eng + SRE |
| 30af | **Product analytics + feature flags: PostHog + GrowthBook wiring** | FE | Wk 10 | Wk 14 | FE Eng |
| 30ag | **Onboarding flows: firm onboarding + user first-run + integration wizard** | Design + FE | Wk 14 | Wk 22 | Product Designer + FE Eng |
| 30ah | **Print / PDF memo design + Puppeteer rendering pipeline** | Design + BE | Wk 16 | Wk 24 | Product Designer + Backend |
| 30ai | **Internationalization scaffolding (next-intl, en-US shipped)** | FE | Wk 12 | Wk 16 | FE Eng |
| 30aj | **Microinteractions / motion design (Framer Motion) + reduced-motion compliance** | FE + Design | Wk 18 | Wk 26 | FE Eng + Product Designer |
| 30ak | **Tablet responsive design + iPad-Pro-landscape testing** | FE + Design | Wk 22 | Wk 30 | FE Eng + Product Designer |
| 30al | **Usability testing rounds with pilot firms (3 rounds: wk 22, 28, 33) + iteration** | Design + UX Res | Wk 22 | Wk 35 | Product Designer + UX Researcher |
| 30am | **Final accessibility audit + remediation pass before launch** | A11y + FE | Wk 35 | Wk 38 | A11y Consultant + FE Eng |
| 31 | Judge-attorney compatibility + lead-attorney optimization | Python + Rust | Wk 17 | Wk 20 | ML / Sim Eng |
| 32 | Expert-witness selection module | Python + Rust | Wk 18 | Wk 20 | Sim Eng |
| 33 | Datalog rule engine + initial federal/CA/NJ rule corpus | Python | Wk 10 | Wk 18 | Logic |
| 34 | Z3 SMT integration | Python | Wk 14 | Wk 19 | Logic |
| 35 | Argumentation framework (Dung/ASPIC+) | Python | Wk 16 | Wk 20 | Logic |
| 36 | OWL ontology + description-logic reasoner | Python | Wk 15 | Wk 19 | Logic |
| 37 | Temporal + deontic logic | Python | Wk 17 | Wk 20 | Logic |
| 38 | Dempster–Shafer evidence aggregation | Python | Wk 18 | Wk 20 | Logic |
| 39 | State-space / HMM + process mining (PM4Py) | Python | Wk 17 | Wk 21 | ML / Logic |
| 40 | **Property-based tests** — `hypothesis` (Python) + `proptest` (Rust) for rule engine + feature store + cost engine + sim engine | both | Wk 19 | Wk 23 | QA |
| 41 | **Rust decision-arithmetic core** — EV / CVaR / Nash inner loops | Rust | Wk 18 | Wk 22 | Rust Eng / Sim Eng |
| 42 | **Rust distributional cost-of-litigation engine** | Rust | Wk 19 | Wk 22 | Sim Eng |
| 43 | **Rust Monte Carlo trial simulation engine** — `rayon` + `ndarray` | Rust | Wk 20 | Wk 25 | Sim Eng |
| 44 | Decision-layer Python orchestrator (calls Rust services) | Python | Wk 18 | Wk 23 | Backend |
| 45 | Stochastic DP / Bellman optimal-stopping | Python | Wk 21 | Wk 23 | Backend |
| 46 | Robust optimisation (cvxpy) | Python | Wk 22 | Wk 24 | Backend |
| 47 | Compliance: protected-class proxy audit + lineage tracker + per-tenant toggles | Python + Rust | Wk 13 | Wk 23 | Compliance |
| 48 | Compliance: disparate-impact report generator | Python | Wk 19 | Wk 24 | Compliance |
| 49 | Federated learning coordinator (Flower) + DP-SGD (Opacus) | Python | Wk 19 | Wk 26 | Federated |
| 50 | **Rust secure-aggregation transport** for federated-learning traffic | Rust | Wk 22 | Wk 27 | Rust Eng / Federated |
| 51 | Federated learning: privacy accountant + membership-inference attack tests | Python | Wk 23 | Wk 27 | Federated / QA |
| 52 | **Rust real-time event broker** — tokio + WS fan-out | Rust | Wk 22 | Wk 26 | Rust Eng |
| 53 | Adversarial robustness — `textattack` (Python) + `cargo fuzz` (Rust) | both | Wk 23 | Wk 27 | QA |
| 54 | Frontend: intake (cause-aware), workspace core, factor breakdown, rule trace | TS | Wk 12 | Wk 25 | Frontend |
| 55 | Frontend: counterfactual + sensitivity + comparables + Nash/Rubinstein panel | TS | Wk 20 | Wk 27 | Frontend |
| 56 | Frontend: Monte Carlo + cost breakdown + lead-attorney + expert | TS | Wk 23 | Wk 29 | Frontend |
| 57 | Frontend: compliance disclosure + admin proxy-audit + federated-learning dashboard | TS | Wk 24 | Wk 29 | Frontend |
| 58 | Firm admin + audit log + PDF memo export | TS | Wk 26 | Wk 30 | Frontend |
| 59 | **Partner API on Rust gateway**: GraphQL + REST + webhooks + OAuth 2 | Rust | Wk 25 | Wk 30 | Rust Eng / Partner Eng |
| 60 | Partner integrations: Clio + MyCase + NetDocuments adapters | TS / Python | Wk 28 | Wk 33 | Partner Eng |
| 61 | QA: tests, calibration, conformal coverage, compliance, adversarial, cross-plane | both | Wk 29 | Wk 35 | QA |
| 62 | Security audit + pen test + SOC 2 readiness review | both | Wk 31 | Wk 36 | DevOps |
| 63 | Production deployment + pilot-firm onboarding | both | Wk 36 | Wk 38 | Ops |

## 16. Team Assignments

> **v2.14 cleanup note:** earlier spec versions (v1.0 → v2.13) used a "NEW HIRE" framing inherited from the GigForge consultancy v1.0 spec, where outside hires would have staffed each role. This project is built by the existing **GigForge agent team** (Claude Sonnet for lead engineering / PM / QA / dev-frontend / dev-backend; local Gemma 4 for routine cron-driven work; specialist agents for design / DevOps / legal). No outside hires are required for the work itself — only for genuinely human-bound activities (pilot-firm scheduling, real legal-SME sign-off on jurisdiction-specific rule encoding, real cloud-account provisioning).
>
> The table below maps each spec-named role to the agent that actually executes it.

| Function | Agent (or human) | Notes |
|----------|-----------------|-------|
| Operations Director | Alex Reeves (real-world placeholder; in practice Braun directly) | Overall delivery + owner-side decisions |
| Project Manager / Scrum Master | `gigforge-pm` (Jamie Okafor persona) | Sprint planning, backlog, TDD/BDD enforcement, daily + weekly reports |
| Lead Engineer / CTO + Rust Data Plane | `gigforge-engineer` (Chris Novak persona) | Architecture, all Rust services (gateway, feature-store, ingestion, simulation, cost engine, decision-arith, event broker), gRPC contracts. The "Senior Rust + Distributed Systems Engineer" role from v2.4 is this same agent. |
| Backend Engineer | `gigforge-dev-backend` | Python services other than ML, partner-API integrations, Django back-office app |
| Frontend Engineer | `gigforge-dev-frontend` | Next.js workspace, Apollo client, design-system implementation against `gigforge-ux-designer`'s tokens |
| AI / ML Engineer | `gigforge-dev-ai` | Subsumes the v2.1+ Graph/GNN, Causal Inference, Logic/Knowledge, NLP/Personality, Federated Learning, Simulation/Decision specialist roles. Sub-domains routed by task, not by agent. |
| DevOps + SRE | `gigforge-devops` (Casey Muller persona) | CI/CD, security audits, build pipelines, K8s, GitOps, observability, on-call. The "Senior SRE / Platform Engineer" role from v2.8 is this same agent. |
| QA + Accessibility | `gigforge-qa` (Riley Svensson persona) | Tests, calibration, coverage, compliance audits, adversarial, property-based. Subsumes the v2.11 Accessibility Consultant role — a11y is a QA discipline. |
| Customer Delivery / CSM | `gigforge-csm` + `gigforge-cs-advocate` (Jordan Whitaker persona) | Pilot-firm onboarding, feedback loop |
| Product Designer + UX Researcher | `gigforge-ux-designer` | Design system, IA, visual design, interaction design, data viz language, persona development, partner interviews. Subsumes both v2.11 roles. |
| Compliance / Privacy | `gigforge-engineer` + `gigforge-legal` pairing | Type-system enforcement is engineering work; policy + proxy-audit interpretation is legal review |
| Legal SME (Federal + CA + NJ) | `gigforge-legal` + `gigforge-legal-assoc-1` + `gigforge-legal-assoc-2` + the actual client (Drazen Komarica) for sign-off on jurisdiction-specific rule encoding | The agents draft + cross-check; a real-world attorney must sign off before rule-engine output ships to a tenant |

### What "human time" actually buys us in Phase 1

The 41-week Phase-1 timeline in §15 was sized assuming human hires. With the agent team, agent capacity is sub-day per story (Sprint 1 demonstrated 14 stories executed in day 1). The actual Phase-1 bottlenecks are:

- **Cloud-account provisioning** for the EKS cluster (real money, real human approval).
- **Pilot-firm scheduling** — partner interviews, beta access, contract signatures.
- **Legal-SME sign-off** on every jurisdiction-specific rule encoding before it can be promoted to staging or prod.
- **Code review and SOW gating decisions** that can't be auto-approved.

Realistic re-projection: Phase 1 could land in **8–16 weeks** (not 41) if the above human-bound dependencies move at reasonable cadence. The §15 milestone numbering is preserved as a reference but the wall-clock should be re-calibrated against actual cloud + pilot-firm progress at the end of Sprint 2.

## 17. Deliverables

(unchanged in capability from v2.3; selected deltas noted)

| Deliverable | Format | Responsible |
|-------------|--------|-------------|
| Production multi-tenant web application | Deployed URL + per-tenant creds | Chris / Casey |
| Source code (mono-repo with `rust/` and `python/` workspaces) | Private repo | Chris / Rust Eng |
| Architecture & data model docs (incl. polyglot ADR) | Markdown + diagrams | Chris |
| **Rust crates: api-gateway, feature-store, ingestion, sim-engine, cost-engine, event-broker, decision-arith** | Cargo workspace | Rust Eng |
| **`.proto` contracts + buf schema registry config** | Repo + CI | Backend |
| Heterogeneous KG schema + Neo4j load scripts + network-analytics jobs | Cypher + JSON-LD | Graph Eng |
| Trained GNN weights + KG embeddings | PyTorch + pgvector | Graph Eng |
| Personality-extraction LoRA + eval | LoRA + PDF | NLP |
| Topic-modelling artifacts | BERTopic models + corpus | NLP |
| Tabular-PDF extraction pipeline | Python + tests | NLP |
| Self-consistency verifier | Python module | NLP |
| Ideology / demographic ingestion pipelines | Rust + Python | Rust Eng / Backend |
| Rule corpus + version history | Datalog/JSON + change log | Logic + SME |
| Argumentation framework definitions | Custom DSL + JSON | Logic + SME |
| OWL ontology of legal concepts | OWL/XML | Logic + SME |
| Process-mining models | PM4Py + JSON | Logic + ML |
| Property-based test suites | `hypothesis` + `proptest` | QA + Logic + Rust Eng |
| Fine-tuned Gemma 4 LoRA (judicialpredict-en) | LoRA + eval | ML |
| ML model registry + runbooks | MLflow + Markdown | ML |
| Causal inference report (DAGs, IV, refutation tests) | PDF + notebooks | Causal |
| Conformal prediction coverage audit + per-stratum reliability | PDF | QA |
| Counterfactual explanation library | Python + tests | ML |
| Two-stage settlement-history model | Python + eval | ML |
| Concept-drift monitoring service | Python + dashboard | ML |
| **β-VAE imputation model + uncertainty propagation harness** | PyTorch + tests | ML |
| **CEVAE estimator + sensitivity report** | Python + PDF | Causal |
| **Domain-adversarial training harness + jurisdiction-invariance audit** | Python + report | ML |
| **TabDDPM-with-DP synthetic-data generator + privacy disclosure** | Python + PDF | Federated / ML |
| **Django admin / back-office app** (tenant mgmt, rule editor, audit log, lineage, FL dashboard, proxy-audit, disparate-impact, partner token mgmt) | Django app + tests + docs | Django Eng |
| **Django staff SSO + permission policies + audit-log of admin actions** | Django + OIDC | Django Eng |
| **`ml-inference-svc` + horizontal-scaling Helm chart + p99 SLO dashboard** | Python service + Helm | ML + DevOps |
| **`ml-training-job` Airflow/Argo DAGs + GPU-pool config + champion/challenger gating** | Workflows + infra | ML + DevOps |
| **`llm-client-svc` (httpx pool + retry + rate-limit + circuit breaker + prompt cache + Gemma 4 SLA dashboard)** | Python service + ops docs | NLP + DevOps |
| **`ingest-fetcher` + `feature-deriver` Rust binaries + replay tooling + Redis-Streams contract** | Rust + tests | Rust Eng |
| **`partner-gateway` Rust binary + OAuth 2 scopes + per-partner rate-limit + abuse-monitoring + revocation tooling** | Rust + ops docs | Rust Eng + Partner Eng |
| **K8s cluster manifests + Helm charts per service + ArgoCD App-of-Apps gitops/ tree** | Helm + YAML + ArgoCD | SRE |
| **CI pipeline (lint + build + test + Trivy + Syft + Cosign + GHCR + gitops PR)** | GitHub Actions YAML | SRE + DevOps |
| **Argo Rollouts canary policies + Prometheus metric gates + auto-rollback runbooks** | YAML + runbook | SRE |
| **Argo Workflows DAGs (training, migrations, retrains) + GPU-pool config** | Workflow YAML | SRE + ML |
| **Observability stack (Prometheus + Grafana + Loki + Tempo + Alertmanager) + per-service SLO dashboards** | Helm + dashboards | SRE |
| **External Secrets Operator + KMS/Vault wiring + secret-rotation runbook** | YAML + runbook | SRE |
| **DB migration scaffolding (sqlx-migrate + Alembic + Django migrate) as Argo Workflows preflight** | Workflow + tooling | SRE + Backend |
| **Design system v1 (Figma library + Storybook + design tokens + customised shadcn/ui)** | Figma + Storybook + TS tokens | Product Designer + FE Eng |
| **Persona docs + UX research findings + usability-testing reports** | Markdown + video clips | UX Researcher |
| **Information architecture + wireframes (`judicialpredict-wireframes.md`) + high-fidelity Figma mockups** | Markdown + Figma | Product Designer |
| **Brand identity package (logo + palette + typography + voice/tone guide)** | Brand book + Figma | Product Designer |
| **Data viz design language doc** | Markdown + Figma examples | Product Designer |
| **Accessibility audit reports + WCAG 2.2 AA compliance certification** | PDF + remediation log | A11y Consultant |
| **Performance budgets + Lighthouse CI config + RUM dashboards** | YAML + Grafana | FE Eng + SRE |
| **Product analytics event schema + PostHog dashboards + feature-flag inventory** | Schema + dashboards | FE Eng |
| **Onboarding flow specs + implementation (firm + user + integration)** | Design + code | Product Designer + FE Eng |
| **PDF memo template + rendering pipeline + sample outputs** | Print stylesheet + Puppeteer + samples | Product Designer + Backend |
| **i18n scaffolding + en-US message catalogue** | next-intl + JSON | FE Eng |
| **Motion design specs + Framer Motion implementation** | Figma + code | Product Designer + FE Eng |
| **ADR-FP-001: Functional-core / imperative-shell paradigm boundaries** | Markdown ADR | Chris Novak + Rust Eng + Logic Eng |
| **`decision-arith` crate (Rust, pure functions, property-tested invariants)** | Cargo crate + proptest suite + algebraic-invariant docs | Rust Eng + Sim Eng |
| **`monte-carlo-sim` crate (Rust, pure trajectory closures, rayon parallel)** | Cargo crate + property tests + benchmarks vs simpy baseline | Sim Eng + Rust Eng |
| **`cost-engine` crate (Rust, distributional composition pure)** | Cargo crate + property tests | Sim Eng + Rust Eng |
| **`feature-store-types` crate (Rust ADTs + exhaustive match for Tier/Sensitivity/PermittedUse)** | Cargo crate + compile-time-rejection tests | Compliance Eng + Rust Eng |
| **Logic-service rule-application pure functions (Python, immutable fact bases)** | Python module + Hypothesis tests | Logic Eng |
| **Causal-inference estimator wrappers (Python, pure)** | Python module + Hypothesis tests | Causal Specialist |
| **Polars LazyFrame ML data pipelines (immutable transformations)** | Python module + benchmarks | ML |
| **Frontend state architecture (Zustand or Redux Toolkit, pure reducers, hooks-only)** | Code + ADR | FE Eng |
| Fuzzy MF library + documentation | Python module + spec | ML |
| Decision-layer reference implementation (Python orchestrator + Rust core) | Code + spec | Backend / Rust Eng |
| Distributional cost-of-litigation engine | Rust crate + tests | Sim Eng / Rust Eng |
| Monte Carlo trial simulation engine | Rust crate + tests | Sim Eng / Rust Eng |
| Lead-attorney + expert-witness optimization modules | Python + UI | Sim Eng |
| Feature-store with tier + sensitivity + lineage schema | Rust crate + Postgres + API + docs | Compliance / Rust Eng |
| Protected-class proxy-audit job + dashboard | Python + UI | Compliance |
| Disparate-impact report generator + templates | Python + PDF templates | Compliance |
| Compliance policy document | Markdown / PDF | Compliance + SME |
| Federated learning coordinator + DP-SGD training | Python + infra | Federated |
| Privacy accountant + compliance disclosure | Python + PDF | Federated |
| Secure-aggregation transport | Rust crate | Rust Eng / Federated |
| Adversarial robustness regression suite | `textattack` (Python) + `cargo fuzz` (Rust) | QA |
| Automated test suite (unit + e2e + cross-plane integration) | pytest + Playwright + cargo nextest | QA |
| Calibration + audit reports | PDF | QA |
| Compliance audit test suite | pytest + proptest | QA |
| Partner API (GraphQL + REST + webhooks) + OAuth 2 | Rust crate + spec + reference impl | Rust Eng / Partner Eng |
| Partner integrations (Clio + MyCase + NetDocuments) | Adapters + tests | Partner Eng |
| Public partner-API documentation + sample code | Web docs + Postman | Partner Eng |
| CI/CD + Helm charts | GitHub Actions + K8s YAML | DevOps |
| Operator runbook (incl. polyglot ops + cross-plane debugging) | Markdown / PDF | DevOps |
| User documentation | Web docs site | PM |
| Pilot-firm case-evaluation memo template | PDF template | Customer Delivery |

## 18. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Ground-truth case-outcome data sparse for state cases | High | Medium | Hierarchical Bayes + conformal CIs; lean on federal CAP/CourtListener |
| Heterogeneous-graph training is engineering-heavy | Medium | High | Dedicated Graph Eng; staged R-GCN → HGT → TGN |
| Causal-inference assumptions can't be verified | Medium | Medium | DoWhy refutation tests; surface assumptions in UI |
| Conformal coverage degrades under distribution shift | Medium | Medium | Continuous coverage monitoring per stratum; nightly recalibration |
| Fuzzy MF parameters drift from legal-community consensus | Medium | Medium | SME quarterly review; per-firm overrides |
| LLM hallucinations in element extraction | Medium | High | Schema-validated outputs; HITL; active learning; self-consistency; LLM never drives recommendation directly |
| Rule engine misencodes a statute | Low | High | Source-cited rules; SME sign-off; rule unit tests; property-based testing |
| Argumentation framework becomes tangled | Medium | Medium | Editorial process; ontology-typed arguments; visualisation |
| Tenant data leakage | Low | Severe | RLS; per-tenant keys; pen test before pilot |
| Federated learning leaks information despite DP | Low | Severe | Formal (ε, δ)-DP; membership-inference attack tests; published privacy parameters; opt-in only |
| Federated learning model performance degrades | Medium | Medium | Champion/challenger; tenant-level toggle |
| Unauthorized practice of law claims | Low | High | Disclaimers; firm-only access; ToS by counsel |
| Free data sources change ToS | Low | Medium | Local mirrors; adapter layer |
| Calibration / coverage drift in production | Medium | Medium | Continuous monitoring; champion/challenger; per-feature drift detection |
| Title VII / disparate-impact exposure from protected-class proxy features | Medium | Severe | Feature-tier enforcement; mandatory proxy-audit jobs; flagged features blocked until reviewed |
| Personality / ideology features perceived as unfair profiling | Medium | High | Well-established academic measures; cite sources in UI; per-tenant toggle |
| Reputational risk from misuse of demographic features | Low | Severe | Hard block on Tier-C predictive use; quarterly external review |
| Feature lineage gaps | Low | High | Every feature must register in feature-store; CI gate prevents unregistered features |
| Monte Carlo simulation diverges from observed reality | Medium | Medium | Calibrate against historical case outcomes; backtesting in QA |
| Adversarial inputs flip recommendations | Medium | High | textattack regression suite; flag adversarial signatures at intake; per-feature attribution audit |
| Counterfactual explanations expose model gaming surface | Medium | Medium | Limit features counterfactuals operate over; audit-log every counterfactual query |
| Partner-API misuse by integration partners | Low | Medium | Scoped OAuth tokens; rate-limit; abuse monitoring; revocation |
| Timeline expansion driven by methodological + compliance + partner breadth | High | Medium | Phase-2 backlog firmly delineated; ruthless prioritization |
| **Rust hiring market scarcity** | **Medium** | **High** | **Begin recruiting at v2.4 sign-off; secondary candidates trained from senior Python engineers via 8-week onboarding; advisory help from Rust consultants if no permanent hire by Wk 4** |
| **Compile-time impact on CI** | **Medium** | **Medium** | **`sccache` + `cargo nextest` + parallel CI workers; benchmark CI wall-clock at every milestone; cap clean-build CI at 15 min** |
| **Cross-language debugging complexity** | **Medium** | **Medium** | **OpenTelemetry distributed tracing across both planes; correlation IDs threaded through every gRPC call; runbook for cross-plane incidents** |
| **gRPC schema drift between Rust and Python sides** | **Medium** | **High** | **`buf` lint + `buf breaking` in CI; cross-plane integration tests on every `.proto` change; semantic-versioned proto packages** |
| **Rust ecosystem gaps for legal-domain libraries** | **Low** | **Medium** | **Selective Rust scope — never Rust where Python ecosystem dominates; documented per-service language rationale; if a Rust-side need arises with no mature crate, fall back to Python service or HTTP-API access pattern** |
| **Polyglot operational overhead** | **Medium** | **Medium** | **Single mono-repo; shared CI; shared observability; clear ownership per service; runbook documents per-language ops conventions** |
| **Synthetic data drifts from real legal patterns and biases pre-training** | **Medium** | **Medium** | **TabDDPM data used only for pre-training augmentation, never as primary training source; final training + evaluation on real cases only; statistical-fidelity audits comparing synthetic-vs-real feature distributions per release; per-tenant opt-out** |
| **VAE imputation introduces correlated errors that conformal coverage misses** | **Low** | **Medium** | **Imputation uncertainty propagated explicitly into conformal stratification; per-feature missingness audit; ablation against MICE / mean-imputation baselines reported in calibration audit** |
| **CEVAE latent-confounder identifiability assumptions violated** | **Medium** | **Medium** | **CEVAE results presented alongside (not replacing) DoWhy / IV / propensity estimates; agreement-of-methods reporting; sensitivity analysis on latent-dim and prior choices** |
| **Django admin schema drift if Postgres schema evolves** | **Medium** | **Low** | **Unmanaged models + `inspectdb` regen on schema migration; CI gate runs Django startup + admin smoke tests on every schema change; clear ownership: schema is owned by Rust feature-store + Python ML services, never Django** |
| **Django admin actions bypass compliance enforcement** | **Low** | **High** | **Django routes all mutations through gRPC to Rust feature-store; direct Postgres writes from Django are blocked by Postgres role permissions (Django role is read-only on policy-relevant tables); admin-action audit log mandatory** |
| **Two auth surfaces (customer JWT + staff OIDC) cause confusion** | **Low** | **Low** | **Documented in runbook; separate domains (`app.judicialpredict.com` vs `admin.judicialpredict.internal`); staff SSO routed through corporate IdP** |
| **v2.7 service splits add operational surface (5 new processes)** | **Medium** | **Medium** | **Each split has documented operational reason (different scaling profile / deployment cadence / failure isolation / security boundary); shared CI + observability stack absorbs marginal cost; runbooks per service; refuse further splits without similar justification** |
| **`ml-inference-svc` ↔ `ml-training-job` model-handoff drift** | **Low** | **High** | **MLflow champion/challenger as the only path from training to serving; integration test gate verifies served model matches the registry hash; rollback to last known-good is one-flag** |
| **Gemma 4 SLA degradation cascades through `llm-client-svc`** | **Medium** | **Medium** | **Circuit breaker + degraded-mode response (skip LLM-driven extraction, use spaCy-only fallback); per-tenant quota; alerts on Gemma 4 latency p99** |
| **`partner-gateway` security gap exposes admin paths** | **Low** | **Severe** | **Separate process boundary blocks code-path mistakes; partner-gateway has zero admin scopes by design; pen-test specifically targets the partner gateway** |
| **Replay-from-`ingest-fetcher` corrupts feature store on schema migration** | **Low** | **Medium** | **Schema-version stamps on every raw blob; `feature-deriver` refuses to write across schema-version mismatches without explicit migration flag; replays exercised in QA on every release** |
| **K8s operator dependency lock-in (CloudNativePG, Neo4j, Redis, MinIO operators)** | **Low** | **Medium** | **Operators chosen for active maintenance + LTS commitments; quarterly review of operator health; documented escape paths to managed services if any operator is abandoned** |
| **ArgoCD CRD breakage on K8s upgrade** | **Medium** | **Medium** | **Pin ArgoCD to LTS releases; staging cluster always one minor version ahead of prod; release notes reviewed before each K8s upgrade** |
| **GPU-pool cost overruns** | **Medium** | **Medium** | **Spot instances where workload tolerates interruption; nightly-retrain windows tuned to off-peak pricing; weekly cost review; per-tenant fair-use quotas on fine-tune calls (Phase 2)** |
| **Canary metric gates falsely block rollouts on unrelated regressions** | **Medium** | **Medium** | **Per-rollout metric scoping (only metrics specific to changed service contribute); manual override path documented; runbook for "rollout stuck due to noisy metric"** |
| **Promotion-as-PR introduces coordination friction at pace** | **Medium** | **Low** | **Pre-approved auto-merge for routine bumps (image-tag-only PRs from CI); manual approval reserved for chart/values changes; documented escalation path** |
| **Secret-rotation gaps cause incidents** | **Low** | **High** | **External Secrets Operator with refresh interval; rotation runbook; quarterly rotation drill; alert if any secret older than policy threshold** |
| **UX research timing pushes frontend implementation late** | **Medium** | **Medium** | **Research + IA + design system run weeks 1–8 in parallel with backend work; FE implementation accelerates once design system v1 lands wk 8; usability testing rounds time-boxed and budgeted into the 41-week plan** |
| **Design system + brand work overruns into pilot launch** | **Medium** | **Medium** | **Design system v1 is shipped at wk 8 — partial but sufficient for FE acceleration; brand work continues in parallel without blocking; "ship with current design + iterate" explicitly preferred over "wait for perfect"** |
| **Pilot firms reject design as "looks like consumer SaaS, not a serious legal tool"** | **Medium** | **High** | **PDF memo design intentionally Cravath-grade; partner-facing screens calibrated through 3 usability testing rounds with actual partners; voice & tone guide enforces "calibrated, not boastful"; brand identity targets serious-legal aesthetic** |
| **WCAG 2.2 AA gate fails enterprise procurement** | **Low** | **High** | **A11y consultant on retainer from wk 6; axe-core CI gate from wk 8; manual screen-reader passes on every release; quarterly external audits; remediation budget reserved in Phase 1 timeline** |
| **Performance budget regressions accumulate as workspace grows** | **Medium** | **Medium** | **Lighthouse CI budget enforcement on every PR; RUM monitoring per-tenant performance; per-quarter budget review with stakeholder sign-off on any planned regression** |
| **Onboarding abandonment** | **Medium** | **High** | **Three distinct onboarding paths designed + analytics-instrumented; PostHog funnels with abandonment alerts; iteration based on real abandonment data starting at pilot wk 22** |
| **Microinteractions ignored by `prefers-reduced-motion` users** | **Low** | **Medium** | **Framer Motion respects `prefers-reduced-motion` by default; explicit accessibility tests with reduced-motion enabled; falls back to instant transitions** |
| **i18n scaffolding rots without active locales** | **Low** | **Low** | **Even en-US catalogue surfaces missing-key bugs in CI; i18n-tooling validation in CI prevents regressions; periodic dry-run with a synthetic second locale to catch hardcoded strings** |
| **Dogmatic FP applied where state is genuine** | **Medium** | **Medium** | **ADR-FP-001 enumerates designated functional-core vs imperative-shell services explicitly; "use a class when state is genuine" rule enforced in code review; no monad-transformer towers; pragmatic FP, not category theory** |
| **Engineers used to OOP write Java-in-Rust or Java-in-Python** | **Medium** | **Medium** | **Pair programming pairs senior Rust eng with newcomers; ADR-FP-001 onboarding doc; code-review checklist includes "are we fighting the language?"; Rust style guide explicitly references functional idioms (iterators over loops, Option/Result over sentinel values, exhaustive match)** |
| **Pure-function discipline in Python produces unfamiliar verbosity** | **Low** | **Low** | **Accept verbosity over mutable shortcuts on legal-prediction code where correctness compounds; Polars LazyFrame ergonomics + Pyrsistent / `frozendict` for immutable collections make idiomatic FP-Python tractable** |
| **Debugging deeply-nested functional pipelines is harder than imperative** | **Low** | **Medium** | **Polars `.collect_schema()` + lazy-plan introspection; Rust `tracing::instrument` spans on functional-core entry points; observability investment from week 1 (§11.5) covers both planes** |

## 19. Future / Phase 2 Methods

(unchanged from v2.3 — RLHF, bandits, knowledge distillation, per-tenant LoRA adapters, mean-field game theory, full Bayesian-optimisation MF tuning, multi-modal audio, OCR scanned filings, mobile, e-filing, multi-jurisdiction expansion, expanded fairness suite, voir-dire behind ethical-review gate)

### Possible Phase 2 Rust expansions

- **Rust ML inference** for very latency-sensitive paths (e.g., live-event-driven recompute) using `tract` or ONNX runtime in Rust, only after Python serving proves the bottleneck.
- **Rust-side rule engine** if `crepe` (Rust Datalog) matures enough to host the Phase 1 rule corpus with ASP-equivalent expressiveness.
- **Per-tenant Rust extensions** — sandboxed WASM modules running tenant-specific rules at the gateway, isolated from the Rust core process. Architecturally feasible; demand-driven.

## 20. Commercial Summary & Next Steps

### Pricing — Indicative

Premium tier (AI & Automation), substantially expanded scope vs v1.0. Fixed-price proposal after open items below.

### Open Items

- Pilot-firm count for Phase 1 launch.
- Per-tenant rate-limit / fair-use targets.
- Fine-tune cadence (one-shot at launch vs continuous).
- Federated-learning launch tenant set.
- Default global-policy posture for feature-tier toggles.
- Partner-API launch partners (Clio, MyCase, NetDocuments — order and depth).
- Phase-2 methodological commitments vs continued parking.
- Voir-dire ethical-review pre-conditions and timing.
- **Rust headcount: one senior hire vs two mid-level hires; remote-friendly vs on-site.**
- **gRPC vs alternative cross-plane transport** (gRPC is the strong default; revisit only if a specific operational need surfaces).

### Immediate Next Steps

1. Client review of this v2.4 spec — annotated feedback within 5 business days.
2. Kickoff call to resolve open items above.
3. Data audit: confirm CourtListener/CAP/CALI/MQ/JCS/Bonica/FEC/FJC access and sizes.
4. RunPod capacity check for two new English LoRA fine-tunes.
5. Recruit: Senior Rust + Distributed Systems Engineer (start week 1), Graph Engineer, Causal Specialist, Logic Engineer, Compliance Engineer, NLP / Personality Engineer, Federated Learning Researcher, Simulation Engineer, Partner Integrations Engineer, Legal SME.
6. Initial outreach to Clio / MyCase / NetDocuments partner programs.
7. Initial outreach to law schools for academic-partnership data agreements.
8. Architectural Decision Record (ADR) drafted documenting the Rust / Python boundary and the gRPC contract policy.
9. SOW issued post-kickoff.
10. Sprint 1 starts on signature.

---

**Contact**
Alex Reeves — Operations Director, GigForge
ops@gigforge.ai · gigforge.ai
