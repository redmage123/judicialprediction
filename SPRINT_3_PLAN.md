# Sprint 3 — Demo Vertical Slice + Real-Data Foundation

**Cycle:** 2026-06-04 → 2026-06-18 (2 weeks)
**Plane cycle:** `bf926eb0-5b16-4e19-a6bc-d33e7ccc410b`
**Issues:** JP-42 through JP-54

---

## Goal

An operator can log in, fill a case-intake form, and get a P(win) + 90 %
conformal CI + settle/try recommendation back. The model is trained on
**real CourtListener tax-court opinions**, not synthetic data. The full
polyglot path is exercised end-to-end:

```
Next.js  ─Apollo──►  api-gateway GraphQL  ─gRPC──►  ml-inference-svc  ───►  Postgres + audit_log
```

## Why this scope

Sprint 2 produced a lot of infrastructure (auth, rate limit, audit, RLS,
tenant overrides, scaffolds) but nothing demonstrable. The biggest leverage
now is wiring the existing pieces into one working slice while replacing the
synthetic model with real data.

## Demo definition of done

1. A live URL on the Hetzner box (`78.47.104.139`) where you can submit one
   case from a browser.
2. The page returns a real prediction with 90 % conformal CI.
3. The settle/try recommendation is shown with three bullets of reasoning.
4. The corresponding row exists in `audit_log` with the right tenant and
   `action="predict.invoke"`.
5. The model artefact behind it was trained on real `case_documents` rows,
   not synthetic data.

---

## Stories

### P0 — Vertical slice (must land for demo)

| #         | Plane | Story                                                              | Owner            | Deps      |
|-----------|-------|--------------------------------------------------------------------|------------------|-----------|
| **S3.1**  | JP-42 | GraphQL `predictCaseOutcome` mutation through the polyglot path     | gigforge-engineer    | S2.2 / S2.11 |
| **S3.2**  | JP-43 | Next.js `/case/new` intake form wired to predictCaseOutcome         | dev-frontend     | S3.1      |
| **S3.3**  | JP-44 | Next.js `/case/[id]` results view with reasoning bullets            | dev-frontend     | S3.4      |
| **S3.4**  | JP-45 | Decision-action layer: P(win)+CI+cost → recommendation + bullets    | gigforge-engineer    | —         |
| **S3.5**  | JP-46 | Cookie-based dev login that mints a gateway JWT                     | dev-frontend     | S2.2      |

### P1 — Real data, real model

| #         | Plane | Story                                                              | Owner            | Deps      |
|-----------|-------|--------------------------------------------------------------------|------------------|-----------|
| **S3.6**  | JP-47 | Live CourtListener fetch: real tax bulk dump → ≥ 1k rows           | dev-ai           | S2.17     |
| **S3.7**  | JP-48 | Train gradient-boosted ensemble on **real** tax-court features     | dev-ai           | S3.6      |
| **S3.8**  | JP-49 | Conformal CI calibrated on held-out tax split, target 90 % ±2 %    | dev-ai           | S3.7      |

### P1 — Admin & compliance follow-ups

| #         | Plane | Story                                                              | Owner            | Deps      |
|-----------|-------|--------------------------------------------------------------------|------------------|-----------|
| **S3.9**  | JP-50 | Django admin real RBAC (drop the dev-superuser scaffold)           | dev-backend      | S2.15     |
| **S3.10** | JP-51 | Django admin `tenant_settings` UI (deferred from S2.12)            | dev-backend      | S2.12     |
| **S3.11** | JP-52 | Wire override-change writes through audit-recorder (deferred S2.12)| gigforge-engineer    | S2.12 / S2.11 |

### P2 — Quality (could-have, slips to Sprint 4 if needed)

| #         | Plane | Story                                                              | Owner            | Deps      |
|-----------|-------|--------------------------------------------------------------------|------------------|-----------|
| **S3.12** | JP-53 | cargo-mutants first-run on `decision-arith`, `monte-carlo-sim`, `feature-deriver` | gigforge-engineer | S2.8      |
| **S3.13** | JP-54 | Frontend a11y CI gate via axe-core                                 | dev-frontend     | S3.2 / S3.3 |

**Total:** 13 stories. P0 (5) + P1 (6) is the realistic landing target.

---

## What's deliberately NOT in Sprint 3

- **Knowledge graph (Layer 0)** and **NLP layer (Layer 2)** — Sprint 4+. The
  demo can use a single model output without them.
- **Real SSO** — dev-cookie login is enough for a Hetzner-internal demo.
- **Federated learning, differential privacy, partner API** — multi-sprint
  initiatives, unblocked from the demo.
- **K8s / ArgoCD / EKS** — deliberately cancelled in Sprint 2 (JP-27 / 28 / 29
  closed with rationale: wrong deployment target — JudicialPredict runs on
  the Hetzner box alongside the rest of AI Elevate via Docker Compose).

---

## Risks

| Risk                                                                 | Mitigation                                                                                       |
|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| **S3.7 real-data training**: CourtListener bulk format may not carry enough structured fields for the 7 Tier-A/B features | Spawn a sub-story for NLP feature extraction; expands scope by ~3-5 days. Flag in sprint review. |
| **S3.4 cost-engine integration**: cost-engine is still a stub crate    | Take a `Decimal` cost as a parameter; full cost-engine wiring is a Sprint-4 follow-up.            |
| **S3.6 live CourtListener fetch**: rate-limits or 5xx during dispatch  | Re-runnable; ON CONFLICT idempotent upsert from S2.17. Flag in runbook.                           |

---

## Dispatch strategy

- **Per-dispatch fresh `--session-id`** stays the rule (waves 3 + 4 confirmed
  this is needed; old rolling sessions cause `prompt-error` + infinite
  ToolSearch loops).
- **gigforge-dev-ai's session is contaminated** (~190 KB) — clean it before
  Sprint 3 starts, or route AI work to a fresh agent.
- **Parallelize within priority bands**, serialize across them. P0 → P1 →
  P2. Within P0, S3.1 ↔ S3.4 ↔ S3.5 are independent; S3.2 needs S3.1 and
  S3.3 needs S3.4.

---

## Out of Sprint 3 (Sprint 4 candidates)

- Knowledge graph schema + first nodes (judges, courts, cases).
- Layer 2 NLP extraction (turn opinion plain text into Tier-A/B features
  programmatically).
- Real SSO (replace dev cookie).
- cost-engine implementation.
- Multi-court ingest (NJ + CA on top of tax).
- Real proxy-audit dashboard view in Django admin.

---

*This plan was scoped 2026-05-09 against the Plane backlog after Sprint 2
wave-4 landed. JP-42 through JP-54 are assigned to the "Sprint 3 — Demo
Vertical Slice + Real-Data Foundation" cycle.*
