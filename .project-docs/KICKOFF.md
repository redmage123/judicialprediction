# JudicialPredict — Project Kickoff Brief

**To:** gigforge-pm (Jamie Okafor)
**From:** Operations (Alex Reeves) via owner instruction
**Date:** 2026-05-07
**Project workspace:** `/opt/ai-elevate/gigforge/projects/judicialpredict/`
**Plane project:** JudicialPredict — JP — id `92ad0116-cbac-4975-ac87-4ea820c0be96` (workspace `gigforge`)
**Plane URL:** http://localhost:8801 (token in `/opt/ai-elevate/credentials/plane.env` → use a fresh token from `api_tokens` table; current configured tokens are stale)
**RAG collection:** `gigforge/engineering` — spec + wireframes ingested (135 + 63 chunks)
**GitForge website:** entry live at `/portfolio/judicialpredict`
**Local repo:** `/home/bbrelin/judicialpredict` (on Braun's workstation, not the production server)
**Spec version:** v2.13

## Mandate

Run JudicialPredict end-to-end as a full Agile project — ADRs through dev, devops, UI/UX, QA — using the GigForge agent team. Owner wants:

1. **Full Agile lifecycle.** Two-week sprints (per spec §11.6.1). Scrum ceremonies (planning, daily standup, mid-sprint refinement / Three Amigos, sprint review, retrospective). Sprint 1 starts immediately.
2. **Kanban board with per-agent sub-boards.** Use Plane modules (or labels) to separate work streams per agent: `pm`, `engineer`, `dev-backend`, `dev-frontend`, `dev-ai`, `devops`, `qa`, `brand-designer`, `creative`, `legal`. Every issue carries the owning agent label and progresses through workflow states.
3. **Workflow states.** Backlog → Ready → In Progress → In Review → In Testing → In Staging → In Production. Issues move through states explicitly; no skipping.
4. **ADRs from day one.** First sprint includes ADR-001 (Polyglot Rust + Python + Django + Next.js boundary), ADR-002 (gRPC contracts as single source of truth), ADR-FP-001 (functional-core / imperative-shell paradigm boundaries — already specified in §11.6.7), ADR-003 (Multi-tenant isolation strategy), ADR-004 (Compliance feature-tier enforcement at Rust type-system boundary). Store under `/opt/ai-elevate/gigforge/projects/judicialpredict/adrs/`.
5. **Regular progress reports.** Daily standup digest in `reports/daily-YYYY-MM-DD.md`; sprint review report in `reports/sprint-NN-review.md`; weekly executive summary in `reports/weekly-YYYY-MM-DD.md` emailed to braun.brelin@ai-elevate.ai via ntfy.
6. **TDD + BDD non-negotiable.** Per §11.6.3 + §11.6.4. Three Amigos for every story before coding. Property-based tests on functional-core crates from sprint 1.
7. **Sync external systems regularly.** Plane (every state change), RAG (after every spec/ADR/handoff change), GigForge website CMS (after every milestone), KG (after every sprint). Cron jobs to be created — see §"Operational Cadence" below.

## Reference materials

- **Spec:** `/opt/ai-elevate/gigforge/projects/judicialpredict/judicialpredict-v2-spec.md` (v2.13 — covers four reasoning layers, polyglot architecture, K8s + GitOps platform, full methodology, UI/UX discipline, FP commitments, quantum sub-layer, psychological methodology stack, federated learning + DP, and the demographic / personality / compliance framework).
- **Wireframes:** `judicialpredict-wireframes.md` — IA, state catalogue, accessibility checklist, performance budgets, voice & tone guide.
- **Plane epics already created:** JP-1 through JP-23 (see Plane). Decompose each into sprint stories during refinement.

## Sprint 1 scope (suggested — refine in planning)

- **JP-22 Recruiting kickoff** — confirm agent assignments (engineer, dev-backend, dev-frontend, dev-ai, devops, qa, brand-designer, creative, legal-assoc); identify gaps requiring external hire.
- **JP-1 Platform** — Kubernetes cluster bootstrap; node pools (`general-pool` + `gpu-pool`); CloudNativePG + Neo4j + Redis + MinIO operators; ArgoCD + initial App-of-Apps; Traefik; External Secrets + Vault.
- **JP-2 Methodology rollout** — Linear (or Jira) workspace + project setup; CI scaffolding (lint + format + test gates); Storybook + Chromatic; axe-core/Pa11y/Lighthouse CI; PostHog + GrowthBook wiring.
- **JP-3 Rust gateway scaffold** — `rust/` workspace; `api-gateway` crate (axum + async-graphql + JWT + RBAC + tenant middleware); `feature-store-types` crate (ADTs for Tier / Sensitivity / PermittedUse); first `.proto` contracts.
- **JP-17 UI/UX research kickoff** — schedule 12-15 contextual interviews with pilot-firm partners; persona development; IA workshops.
- **JP-21 Data ingestion scoping** — confirm CourtListener / CAP / Cornell LII / CA / NJ download access; sample-data audit; storage sizing.
- **ADRs.** Author ADR-001 through ADR-004 (see Mandate §4).

## Sprint cadence

- **Sprint length:** 2 weeks.
- **Sprint 1:** 2026-05-07 → 2026-05-21 (start dates may shift slightly to align with team availability).
- **Daily standup:** 09:00 Berlin time. PM digests into `reports/daily-YYYY-MM-DD.md`.
- **Mid-sprint refinement:** Wednesday week-1 of each sprint. Three Amigos format (PO + Dev + QA).
- **Sprint review + retro:** Friday week-2 of each sprint.

## Per-agent kanban sub-boards (in Plane via labels or modules)

- `pm` — Jamie Okafor (gigforge-pm) — sprint orchestration, story refinement, retros, reports
- `engineer` — Chris Novak (gigforge-engineer) — architecture, ADRs, cross-cutting integration
- `dev-backend` — backend Rust + Python services
- `dev-frontend` — Next.js workspace + Django admin frontend
- `dev-ai` — ML / NLP / GNN / fuzzy / quantum sub-layer
- `devops` — Casey (gigforge-devops) — CI, builds, security audits, release engineering
- `sre` — Senior SRE (NEW HIRE per spec §11.5) — cluster, GitOps, observability, on-call
- `qa` — Riley (gigforge-qa) — TDD/BDD enforcement, property-based + adversarial tests, calibration audits
- `design` — Senior Product Designer (NEW HIRE per spec §12.5) — design system, IA, wireframes, brand
- `ux-research` — UX Researcher — interviews, personas, usability testing
- `legal-sme` — Federal + CA + NJ rule encoding sign-off; Tier-C policy review
- `compliance` — Compliance / Privacy Engineer — feature-store, proxy audit, lineage, DP

## Operational Cadence — external system sync

The following sync jobs must be created and scheduled by gigforge-devops:

- **Every state change in Plane:** RAG re-ingest of relevant ADR / handoff / sprint-board (filter by `path` belongs to `projects/judicialpredict/`).
- **Every commit to local repo:** mirror to a GitHub repo when one is provisioned.
- **Daily 17:00 Berlin:** PM emails daily progress digest to braun.brelin@ai-elevate.ai via ntfy.
- **Weekly Friday 16:00 Berlin:** PM emails executive summary; includes burndown, blockers, ADR list, next-sprint preview.
- **End of every sprint:** GigForge website CMS update (project page badge: "In Development — Sprint N"); RAG re-ingest of all artifacts in `projects/judicialpredict/`; KG entity-update (project status, agent assignments, milestone progress).
- **End of every milestone (epic):** GigForge website CMS milestone-banner update; LinkedIn post draft for marketing review.

Owner of these cron jobs: gigforge-devops with PM coordination.

## Owner contact

Braun (silent owner). Updates via:
- Email: braun.brelin@ai-elevate.ai
- Telegram: weekly executive summary on Fridays
- Plane: Braun has read-only access (request escalation if needed)

## First action

Jamie — read this brief, the spec, and the wireframes. Convene the Sprint 1 planning session. Spawn the engineer (Chris Novak) for ADR-001 authoring. Spawn the dev-frontend agent for Storybook scaffold. Confirm receipt with a status reply within 60 minutes. Sprint 1 starts when the Plane backlog is groomed and agents are dispatched.

— Alex Reeves, Operations Director (acting on owner instruction)
