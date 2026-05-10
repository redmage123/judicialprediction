# Sprint 4 — Demo Polish + Real Persistence

**Cycle:** 2026-06-18 → 2026-07-02 (2 weeks; tentative — adjust to Plane)
**Issues:** JP-55 through JP-67 (13 stories)

---

## Goal

Move the demo from "works for me on a laptop" to "ready to walk a pilot
firm through". The Sprint 3 vertical slice already submits a case and shows
P(win) + recommendation, but everything is sessionStorage-only and the
operator's "I tried 5 cases yesterday" data is gone after a refresh. Sprint 4
fixes that and adds the artefacts a pilot operator actually needs:
persisted cases, a case list, a printable memo, real SSO, an audit-log
viewer.

## Demo definition of done

1. Operator logs in, submits a case from `/case/new`, sees the prediction
   on `/case/[id]`, **closes the browser**, comes back, and the case is
   still there at the same URL.
2. `/cases` lists every case the operator's tenant has run, sortable by date.
3. Each `/case/[id]` has a "Download memo (PDF)" button that produces a
   one-page document with the prediction, conformal CI, recommendation,
   and reasoning bullets — formatted for a senior partner to skim.
4. Real SSO replaces the Sprint-3 dev cookie. Operators authenticate via
   email + password through a real Django auth flow (or the SSO provider
   stub of choice). The dev-cookie path is gone.
5. Audit-log viewer in Django admin lets operators see every prediction
   their tenant has run, who ran it, and when.
6. ml-inference-svc still serves the synthetic-data S2.13 model, but
   training on real CourtListener data is partially exercised once the
   daily-ingest accumulates ≥500 rows (mid-sprint).

## Why this scope

Sprint 3 produced the skeleton. The pieces work end-to-end (10/10 smoke).
But to *show* it to anyone outside the team, the cases need to persist,
the operator needs a list view, and "the math behind the recommendation"
needs to be exportable. Real SSO + audit-log viewer make it pilot-safe.

---

## Stories

### P0 — Persistence + list view (must land for demo)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S4.1**  | `cases` schema migration: input_features jsonb, prediction jsonb, recommendation jsonb, created_by uuid, created_at | gigforge-engineer | —         |
| **S4.2**  | GraphQL `createCase(input: PredictInput!): Case!` — runs predictCaseOutcome internally, persists the row, returns it | gigforge-engineer | S4.1, S3.1 |
| **S4.3**  | GraphQL `listCases(limit: Int, offset: Int): CaseConnection!` — paginated, scoped by JWT tenant | gigforge-engineer | S4.1      |
| **S4.4**  | Replace web sessionStorage stash with createCase mutation; on success, redirect to `/case/<server-id>` | dev-frontend     | S4.2      |
| **S4.5**  | `/cases` list page — table of recent cases with date, name, P(win), recommendation badge | dev-frontend     | S4.3      |

### P1 — Demo artefacts

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S4.6**  | Case-evaluation PDF memo — server-rendered, one page, prediction + CI + recommendation + bullets + footer with model_version + run timestamp | dev-frontend (or backend if SSR-with-headless-chrome) | S4.4      |
| **S4.7**  | "Re-run prediction" on existing case — fetches latest model + writes a new prediction history row | gigforge-engineer | S4.4      |

### P1 — Pilot-firm readiness

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S4.8**  | Real SSO replacing dev cookie. Email+password against the Operator table from S3.9; bcrypt hashing; password reset flow stub. Sprint-5 wires SAML/OIDC | dev-backend      | S3.9      |
| **S4.9**  | Audit-log viewer in Django admin — paginated, filterable by action / actor / tenant. Read-only; super-operators see all tenants | dev-backend      | S3.11     |

### P1 — Real data acceleration

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S4.10** | Email Free Law Project requesting Hetzner-IP allowlist for bulk-data — operations task; track decision + follow-up timeline | (manual)         | —         |
| **S4.11** | Multi-court ingest: extend `courtlistener-daily.sh` to rotate through tax → cafc → bia → scotus daily | gigforge-engineer | S3.6      |
| **S4.12** | S3.7 partial: train ensemble on whatever real data has accumulated by mid-sprint (≥500 rows). Update MODEL_CARD.md with held-out metrics. | dev-ai           | S3.6 daily ingest |

### P2 — Production hardening (could-have, slips to Sprint 5)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S4.13** | HTTP → gRPC for ml-inference-svc; replace reqwest client in api-gateway with a tonic stub against `protos/ml_plane/inference.proto` | gigforge-engineer | S3.1      |

**13 stories.** P0 (5) + P1 (7) is the realistic landing target; P2 (1) slips to Sprint 5 if needed.

---

## What's deliberately NOT in Sprint 4

- **Knowledge graph (Layer 0), Layer 2 NLP, Layer 3 Logic** — Sprint 5+. These are the next conceptual leap; not blocking the demo.
- **Federated learning, differential privacy, partner API (Clio/MyCase)** — Sprint 6+.
- **Real settlement-anchor model** — S3.4's 0.40 heuristic stays; Sprint 5 replaces it once cost-engine is real.
- **cost-engine implementation** — still a stub; Sprint 5.
- **Decision-arith boundary tests** flagged in S3.12 — Sprint 5 cleanup.
- **K8s, CDN, multi-region** — out of scope (Hetzner Compose is the target).

---

## Risks

| Risk                                                                 | Mitigation                                                                                       |
|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| **S4.12 partial training**: needs ≥500 rows of real data to be meaningful; daily ingest at 100/day means mid-sprint at earliest | Build the trainer + held-out script independently; gate the actual training run behind a row-count check. If data is short, train on what's there + synthetic and document the deviation. |
| **S4.6 PDF rendering**: SSR-PDF is fiddly (headless Chrome vs. wkhtmltopdf vs. server-side React-PDF) | Pick one early; document the choice; if it bogs down, ship an HTML "print this page" view as a Sprint-5 prerequisite. |
| **S4.8 real SSO scope**: "real SSO" can mean anything from email+password to full SAML. Stay narrow. | Sprint 4 = email+password against Operator table + bcrypt + password reset stub. SAML/OIDC = Sprint 5. |
| **S4.10 FLP email response time**: weeks, not days | Don't gate Sprint 4 work on it; track in Plane and follow up async. |

---

## Out of Sprint 4 (Sprint 5 candidates)

- Knowledge graph schema + first nodes (judges, courts, cases).
- Layer 2 NLP feature extraction.
- Real settlement-anchor model + cost-engine implementation.
- SAML/OIDC SSO.
- HTTP → gRPC for ml-inference-svc (slipped from S4.13 if it doesn't fit).
- Decision-arith boundary tests + monte-carlo-sim known-vector tests (S3.12 follow-ups).

---

*Scoped 2026-05-10 against the Sprint 3 close. Theme picked: demo polish +
real persistence + pilot-firm readiness, on top of the working vertical slice.*
