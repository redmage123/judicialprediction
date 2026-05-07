# JudicialPredict

Analysis-only case-evaluation suite for US law firms. Given a case file, the system estimates the probability of success at trial, the distribution of damages or sentencing outcomes, the cost and duration of litigation, and the settlement-value range — and returns a defensible **settle / try / borderline** recommendation with the reasoning shown end-to-end.

> **Status:** Specification — v2.13 draft. Implementation has not started.
> **Scope:** US Federal + California + New Jersey, civil + criminal + bankruptcy (federal-only), contract, tort.
> **Mode:** Analysis only. No prediction market. Both-sides framing.

## Architecture (one paragraph)

Polyglot Rust + Python + Django + Next.js. Rust data plane (API gateway, feature store + compliance enforcement, Monte Carlo simulation, ingestion, real-time event broker, decision-arithmetic core, partner gateway). Python ML plane (ML training/inference, NLP, graph ML, logic services, federated-learning coordinator). Django admin app for back-office. Next.js customer app. gRPC across the planes. PostgreSQL + pgvector + Neo4j + Redis + MinIO. Gemma 4 inference reused from existing RunPod with `judicialpredict-en` and `personality-en` LoRA adapters. Shared cluster on Kubernetes with ArgoCD GitOps and Argo Rollouts canary deploys.

## Reasoning stack

1. **Probabilistic / ML** — XGBoost/LightGBM/CatBoost ensembles, hierarchical Bayes (PyMC/NumPyro), causal inference (DoWhy/EconML/IV), conformal prediction (MAPIE), survival models (lifelines), heterogeneous GNN (HGT/TGN/R-GCN/GraphSAGE), KG embeddings (RotatE/ComplEx), VAE imputation, CEVAE, multi-task learning, Bayesian decision networks, mixture-of-experts.
2. **Logic** — Datalog rule engine, Z3 SMT, argumentation frameworks (Dung/ASPIC+), OWL ontology, temporal/deontic logic, Dempster–Shafer, state-space/HMM, process mining.
3. **NLP + Fuzzy** — spaCy + Legal-BERT + Gemma 4 LoRA, fuzzy element-membership (scikit-fuzzy), self-consistency / CoT verification, BERTopic, tabular PDF extraction.
4. **Decision / Action** — EV + CVaR + prospect-theory utility, Nash + Rubinstein + Kalai-Smorodinsky bargaining, anchor-and-adjust + framing-effect models, procedural-justice multiplier, stochastic DP, Monte Carlo trial simulation, lead-attorney + expert-witness optimization, robust optimisation.

Plus: heterogeneous knowledge graph, demographic / personality / compliance framework with Tier-A/B/C/D enforcement, Moral Foundations Theory, HEXACO, cognitive-bias profiling, federated learning + differential privacy + DP-synthetic data via TabDDPM, network analytics (centrality + community detection), psychological-methodology stack, and a quantum / quantum-inspired sub-layer simulated on classical hardware (tensor networks, quantum kernels, QAOA, quantum walks, VQC, amplitude-estimation-inspired importance sampling).

## Engineering methodology

Two-week Agile sprints with full Scrum ceremonies; XP practices (pair programming default, trunk-based development, sustainable pace, YAGNI); TDD red-green-refactor with mutation testing; BDD with Gherkin + Three Amigos + living documentation via Docusaurus; SOLID + DRY (rule-of-three) applied with judgment; pragmatic FP — functional core / imperative shell — with explicit per-service paradigm designations. DevOps culture: DORA metrics, SLO/SLI/error-budget gating, blameless postmortems, weekly chaos experiments via LitmusChaos. WCAG 2.2 AA accessibility from day one.

## Documents

- [`.project-docs/judicialpredict-v2-spec.md`](.project-docs/judicialpredict-v2-spec.md) — Software Specification & Project Plan (v2.13)
- [`.project-docs/judicialpredict-wireframes.md`](.project-docs/judicialpredict-wireframes.md) — Low-fidelity wireframes + IA + state catalogue + a11y checklist + performance budgets + voice & tone guide
- [`.project-docs/judicialpredict-project-plan.pdf`](.project-docs/judicialpredict-project-plan.pdf) — Original GigForge v1.0 (March 2026) — kept for reference

## Repository layout

```
judicialpredict/
├── .project-docs/   # spec + wireframes + reference docs
├── rust/            # Cargo workspace — data plane services
├── python/          # Python services + Django admin app
├── protos/          # gRPC contracts (single source of truth)
├── charts/          # Helm charts per service
├── gitops/          # ArgoCD App-of-Apps + per-env values
│   ├── dev/
│   ├── staging/
│   └── prod/
├── .github/         # CI/CD workflows
├── docs/            # Docusaurus user-facing documentation
├── design/          # Figma exports, design-token sources
├── runbooks/        # Operational runbooks
└── postmortems/     # Incident postmortems
```

## Data sources (all free / open-licensed)

CourtListener / RECAP, Caselaw Access Project (CAP), Cornell LII, CA Courts, NJ Judiciary, US Sentencing Guidelines, CALI eLangdell, Saylor Academy, OpenStax, Federal Judicial Center, Martin–Quinn scores, Judicial Common Space, Bonica DIME, FEC, AmLaw 100.

## Phase 1 timeline

~41 weeks. Multi-team build with explicit service splits and a polyglot Rust + Python architecture.

## License

TBD (proprietary; client engagement under GigForge MSA).

## Contact

Alex Reeves — Operations Director, GigForge — ops@gigforge.ai
