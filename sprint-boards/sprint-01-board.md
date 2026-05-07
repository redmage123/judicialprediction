# JudicialPredict — Sprint 1 Board

**PM:** Jamie Okafor (gigforge-pm)
**Sprint:** 1 — Foundation + Methodology Rollout
**Window:** 2026-05-07 → 2026-05-21
**Plane cycle id:** aa838f21-155a-4a77-a44e-4bd863340c6b
**Plane project id:** 92ad0116-cbac-4975-ac87-4ea820c0be96 (JP)
**Spec:** v2.13

> Pattern follows the legal-prediction-market sprint board format. Update issue states in Plane explicitly via `/api/v1/workspaces/gigforge/projects/{PROJECT}/issues/{ID}/` PATCH with new `state`. Daily standups feed into `reports/daily-YYYY-MM-DD.md`.

## Sprint Goal

Land the foundation — methodology rollout, K8s + GitOps platform bootstrap, Rust gateway + feature-store skeleton, design system v0, ADRs 001–004, UX research kickoff — so Sprint 2 can start on a working CI + GitOps loop with three named ADRs and a calibrated team.

## Sprint Capacity

22 agents nominally available across roles. Sprint capacity will be confirmed in planning ceremony.

## Stories

### S1.1 — Plane sub-board configuration + Three Amigos protocol
**Owner:** Jamie Okafor (gigforge-pm)
**Plane:** subset of JP-2
**Acceptance criteria (Gherkin):**

```
Given the Plane workspace gigforge has the JudicialPredict project
When the PM applies per-agent labels and configures workflow states
Then every issue must carry exactly one agent: label and exactly one priority: label
And the workflow states (Backlog → Ready → In Progress → In Review → In Testing → In Staging → In Production → Cancelled) must exist
And the Sprint 1 cycle must contain ≥ 5 candidate stories before sprint planning concludes
And the Three Amigos protocol must be documented in /opt/ai-elevate/gigforge/projects/judicialpredict/methodology/three-amigos.md
```

### S1.2 — ADR-001 Polyglot architecture boundary
**Owner:** Chris Novak (gigforge-engineer)
**Plane:** subset of JP-2
**Acceptance criteria:**

```
Given the spec v2.13 §7 specifies a polyglot Rust + Python + Django + Next.js architecture
When the engineer authors ADR-001
Then the ADR must document: (1) which services live on the Rust data plane, (2) which on the Python ML plane, (3) the gRPC contract boundary (prost ↔ grpcio), (4) why Rust-vs-Python per service, (5) reversibility (how to migrate a service across the boundary later)
And the ADR must cite specific spec sections (§7, §8.4, §11.6.7)
And the ADR must include a context, decision, status, consequences format
And the file must live at /opt/ai-elevate/gigforge/projects/judicialpredict/adrs/adr-001-polyglot-architecture-boundary.md
```

### S1.3 — ADR-002 gRPC contracts as single source of truth
**Owner:** Chris Novak (gigforge-engineer)
**Plane:** subset of JP-2
**Acceptance criteria:** ADR documents `protos/` as canonical schema location, codegen strategy (prost on Rust, grpcio-tools on Python), `buf` lint + breaking-change CI gates, semantic versioning of proto packages.

### S1.4 — ADR-FP-001 Functional-core / imperative-shell paradigm
**Owner:** Chris Novak (gigforge-engineer) + dev-backend
**Plane:** subset of JP-2
**Acceptance criteria:** ADR enumerates designated functional-core (decision-arith, monte-carlo-sim, cost-engine, feature-store-types, logic-service rule-application, causal estimators), functional-leaning idioms, and imperative-where-state-is-genuine services per spec §11.6.7. Includes property-based-testing requirement on functional-core crates.

### S1.5 — ADR-003 Multi-tenant isolation strategy
**Owner:** Chris Novak (gigforge-engineer) + Compliance Eng (NEW HIRE)
**Plane:** subset of JP-13
**Acceptance criteria:** ADR documents Postgres RLS, per-tenant encryption keys, tenant-scoped pgvector namespaces, optional namespace-per-tenant for regulated tenants (Phase 2), and the federated-learning opt-in/opt-out path.

### S1.6 — ADR-004 Compliance feature-tier enforcement at type-system boundary
**Owner:** Compliance Eng (NEW HIRE) + Rust Eng (NEW HIRE)
**Plane:** subset of JP-13
**Acceptance criteria:** ADR documents Tier-A/B/C/D classification, Rust ADTs for Tier/Sensitivity/PermittedUse, compile-time blocking of Tier-C predictive flow via the type system, runtime audit logging for the rare permitted Tier-C-for-element usage.

### S1.7 — K8s cluster bootstrap + node pool provisioning
**Owner:** Senior SRE (NEW HIRE)
**Plane:** subset of JP-1
**Acceptance criteria:**

```
Given AWS EKS / GCP GKE / Azure AKS is selected at kickoff
When the SRE provisions the cluster
Then there must be two node pools: general-pool (autoscaled CPU) and gpu-pool (taint-isolated NVIDIA T4/L4/A10)
And CloudNativePG, Neo4j Helm chart, Redis Operator, MinIO Operator must be installed and healthy
And Traefik ingress + cert-manager must be operational
And External Secrets Operator must be wired to a cloud KMS or Vault
And ArgoCD App-of-Apps must be deployed with /gitops/dev synced
```

### S1.8 — GitOps repo + Helm chart scaffolds
**Owner:** Senior SRE + DevOps (Casey)
**Plane:** subset of JP-1
**Acceptance criteria:** mono-repo Helm charts for the first 6 services (api-gateway, feature-store, ml-inference-svc, llm-client-svc, ingest-fetcher, feature-deriver) scaffolded; ArgoCD Application manifests in /gitops/dev/; auto-sync proven with a noop deploy.

### S1.9 — CI scaffolding with TDD/BDD/property-based gates
**Owner:** Senior SRE + DevOps (Casey)
**Plane:** subset of JP-2
**Acceptance criteria:** GitHub Actions workflows for lint+format+test+image-build+Trivy+Syft+Cosign+gitops-PR; cargo nextest + sccache; pytest with hypothesis; cucumber-rs + pytest-bdd integration; ≥70% coverage gate; buf lint + buf breaking gates.

### S1.10 — Rust workspace + api-gateway crate skeleton
**Owner:** Senior Rust Eng (NEW HIRE) + Chris Novak
**Plane:** subset of JP-3
**Acceptance criteria:** rust/ Cargo workspace; api-gateway crate with axum + async-graphql + JWT middleware; first .proto contract for healthcheck / case-list; Rust ↔ Python gRPC roundtrip green in CI integration test.

### S1.11 — feature-store-types crate (compile-time tier enforcement)
**Owner:** Senior Rust Eng + Compliance Eng
**Plane:** subset of JP-3
**Acceptance criteria:** Tier (A/B/C/D), Sensitivity (public/quasi-public/inferred/protected), PermittedUse newtype wrappers + ADTs; exhaustive-match enforcement so Tier-C cannot satisfy a non-Tier-C-permitted bound; proptest invariants asserting tier rules cannot be circumvented.

### S1.12 — UX research kickoff (12-15 contextual interviews + persona draft)
**Owner:** UX Researcher (NEW HIRE) + Senior Product Designer (NEW HIRE)
**Plane:** subset of JP-17
**Acceptance criteria:** interview schedule confirmed; 5 persona drafts (Partner / Associate / Paralegal / Ops / Compliance Officer); first interview conducted by end of Sprint 1.

### S1.13 — Design system v0 (tokens + Storybook scaffold)
**Owner:** Senior Product Designer + Frontend Eng
**Plane:** subset of JP-17 + JP-18
**Acceptance criteria:** Figma library with first 12 tokens; customised shadcn/ui shell; Storybook deployed; Chromatic baseline.

### S1.14 — Data ingestion scoping + access confirmation
**Owner:** Backend (Chris Novak) + dev-backend
**Plane:** subset of JP-21
**Acceptance criteria:** sample download from CourtListener, CAP, Cornell LII, CA Courts, NJ Judiciary; storage size estimate per source; adapter contract sketched; sample data in /opt/ai-elevate/gigforge/projects/judicialpredict/data/samples/.

### S1.15 — Recruiting kickoff (5 NEW HIRES + 2 part-time)
**Owner:** Jamie Okafor (gigforge-pm) + Alex Reeves (Operations Director)
**Plane:** JP-22
**Acceptance criteria:** job descriptions drafted for Senior Rust Eng, Compliance Eng, Senior SRE, Senior Product Designer, Django/Back-Office Eng, UX Researcher (0.5 FTE), A11y Consultant (0.25 FTE); first interviews scheduled.

## Definition of Done (Sprint 1)

- All 4 ADRs published in `/opt/ai-elevate/gigforge/projects/judicialpredict/adrs/` with SME sign-off where applicable.
- K8s cluster operational; ArgoCD synced; CI scaffolding green on a smoke commit.
- Rust workspace with api-gateway + feature-store-types crates building in CI.
- 5 personas drafted; first 3 partner interviews complete.
- Sample data ingested from 3 of 5 free sources.
- Sprint review demo on 2026-05-21 16:00 Berlin.
- Retrospective on 2026-05-21 17:00 Berlin; one improvement committed for Sprint 2.

## Open issues

- **Owner blocker:** Gateway auth bridge broken — `openclaw agent --agent gigforge-pm ...` returns HTTP 401. PM dispatch is staged at `PENDING-PM-DISPATCH.md`. Sprint cannot start in earnest until repaired.
- **Hire blocker:** 5 critical hires required before Sprint 2. Operations to escalate to Braun.

## Communication

- **Daily standup:** 09:00 Berlin via the agent-channel; PM digests into `reports/daily-YYYY-MM-DD.md`.
- **Mid-sprint refinement:** Wednesday 2026-05-14 Three Amigos for Sprint 2 stories.
- **Sprint review:** Friday 2026-05-21 16:00 Berlin.
- **Retro:** Friday 2026-05-21 17:00 Berlin.
- **Owner email:** daily-digest 17:00 Berlin via `jp-progress-report.sh daily`; weekly Friday 16:00 Berlin via `jp-progress-report.sh weekly`.
