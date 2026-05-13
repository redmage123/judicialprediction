# Sprint 6 — JudicialPredict

**Theme:** Layers 3 + 4 land, model v1 ships even on a partial corpus, citation
edges close out the KG, and SSO replaces the password-only login path.

**Window:** 2026-05-13 → 2026-06-03 (3 weeks).

---

## Carry-forward from Sprint 5

Two Sprint-5 stories slipped on a hard data dependency (CourtListener daily
cap):

- **S5.1 / S5.2** — model training on real tax-court features + conformal
  CI. Sprint-5 plan budgeted "train on what we have + flag MODEL_CARD as
  v1 retrained when more data accumulates." Sprint 6 picks that up as
  **S6.1 / S6.2** with the v1 framing. Corpus on 2026-05-13: 104 tax-court
  rows + the courtlistener-daily fix from `94b2590` adds ~5/day going
  forward, so by the end of Sprint 6 we expect ~150–200 rows.  The risk
  plan permits training; the gate is "calibration plot in MODEL_CARD honest
  about sample size, not pretend it's prod-ready."

---

## Goals

1. **Layer 3 (NLP) wired in** — legal NER + element classification using
   the EUR-LEX Gemma LoRA (already live on RunPod) extracts richer features
   from `case_documents.full_text_plain` than S5.7's regex pass.  Feeds
   `extractFeatures` as a second-tier suggestion source.
2. **Layer 4 (decision/action) enriched** — recommendation surface now
   carries a sensitivity band and a counter-recommendation when CI is
   wide; Nash-Rubinstein anchor explained in the rationale bullets.
3. **Model v1 trained** — even on a partial corpus.  Honest MODEL_CARD.
4. **KG citation edges populated** from CourtListener `cites` arrays.
5. **OIDC SSO** lands behind the existing password login (operators can
   keep using passwords; firms with IdPs get OIDC).
6. **Quality gates widen** — Pa11y CI, cargo-mutants weekly cron alerting.

---

## Stories

### P0 — Must-have

| #         | Story                                                              | Owner             | Deps      |
|-----------|--------------------------------------------------------------------|-------------------|-----------|
| **S6.1**  | Train gradient-boosted ensemble v1 on current corpus (~150 rows by Sprint end). Replace ml-inference-svc stub. MODEL_CARD.md with held-out + calibration metrics; honestly label v1 sample size. | dev-ai            | S5.1 carry |
| **S6.2**  | Conformal CI calibrated on held-out tax split (target 90% ± 5% — relaxed from S5.2's ±2% due to small corpus). Coverage report in MODEL_CARD. | dev-ai            | S6.1      |
| **S6.3**  | Layer-3 NLP pipeline: spaCy legal NER + Gemma-4 EUR-LEX LoRA element classification on opinion text.  Outputs feed `extractFeatures` as a high-confidence second tier. | dev-ai            | S5.7 NLP, RunPod LoRA |
| **S6.4**  | Layer-4 enrichment: recommendation surface gets a confidence band ("Settle (high conf)" / "Settle (borderline)") + counter-recommendation when CI ≥ 0.20.  Nash-Rubinstein anchor reasoning surfaced in the rationale bullets. | gigforge-engineer | S5.11     |
| **S6.5**  | Populate `case_citations` edges from CourtListener `cites` arrays. Add `cites_json` column to `case_documents`; back-fill from existing rows; populate `case_citations` table for the dev-tenant. | dev-ai            | S5.5 KG, ingest fix |

### P1 — Should-have

| #         | Story                                                              | Owner             | Deps      |
|-----------|--------------------------------------------------------------------|-------------------|-----------|
| **S6.6**  | OIDC SSO via Authlib in Django admin. Successful OIDC callback issues the same JP JWT as password login; operators with a configured IdP get a "Sign in with SSO" button. | dev-backend       | S5.9 auth |
| **S6.7**  | cost-engine v2: layer expected-duration + party-count factors on top of S5.10's jurisdiction-base × motion-count.  Risk-plan called this out as a Sprint-5 simplification to fix. | gigforge-engineer | S5.10     |
| **S6.8**  | createCase BFF accepts an optional `opinion_text` payload — when present, runs `extractFeatures` server-side and stores the suggestion alongside the operator's final values for later NLP-vs-operator accuracy evaluation. | dev-backend       | S5.8      |
| **S6.9**  | courtlistener-daily.sh runs 2 courts per day (current 1) when daily quota allows — back-walk pagination per-court so cafc/bia/scotus also grow.  Updates `jp-courtlistener-daily.log` format. | gigforge-engineer | ingest-fix|

### P2 — Could-have

| #         | Story                                                              | Owner             | Deps      |
|-----------|--------------------------------------------------------------------|-------------------|-----------|
| **S6.10** | A11y CI gate widening — Pa11y + Lighthouse on the 3 main pages (`/login`, `/case/new`, `/cases`); fail CI on any new violation at moderate+ impact. | dev-frontend      | S3.13     |
| **S6.11** | Cargo-mutants weekly cron writes to a real Slack channel on first new survivor; baseline file pinned. | gigforge-engineer | S3.12     |
| **S6.12** | Operator-facing audit log viewer (`/audit` page) — extends S4.9's Django admin viewer to the operator UI behind the `admin` role. | dev-frontend      | S4.9      |

**12 stories.** P0 (5) is the realistic landing target; P1 (4) is the
stretch; P2 (3) slips to Sprint 7 if needed.

---

## What's deliberately NOT in Sprint 6

- **Federated learning + DP** — Sprint 7+. Multi-firm onboarding is a
  prereq.
- **β-VAE / CEVAE / TabDDPM generative methods** — Sprint 7+, blocked on
  larger corpus.
- **Heterogeneous GNN training** — Sprint 7+. KG only has the dev tenant's
  ~100 nodes today; GNN training wants thousands.
- **SAML** (vs OIDC) — Sprint 7+ if a pilot firm asks for it.
- **Layer 2 Logic (Datalog / Z3)** — slipped from earlier sprint planning;
  defer until Layer 3 NLP outputs are stable enough to feed it.
- **Partner API (Clio / MyCase)** — Sprint 8+.

---

## Risks

| Risk                                                                 | Mitigation                                                                                       |
|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| **Corpus still too small for S6.1** even with the ingest fix. | Train v1 anyway; MODEL_CARD documents sample size + calibration plot honestly. Hold the live `/predict` UX behind a tenant feature flag until coverage hits ≥1k. |
| **Layer-3 Gemma LoRA latency** could blow the gateway's 300ms SLA. | Run NLP enrichment async in a background worker that writes back into `case_documents.features_*` columns; `extractFeatures` reads the cached enrichment row.  Synchronous path keeps S5.7's regex tier only. |
| **`cites_json` column rebuild requires a CL re-pull** for the 104 rows already ingested. | Re-pull only changes the schema; the `opinion_id` UNIQUE constraint keeps the upsert idempotent.  Run as a one-off backfill script, not in the daily cron. |
| **OIDC IdP testing without a real provider** — operators can't smoke-test SSO. | Use the Authlib mock IdP (`authlib.integrations.flask_oauth2`) in dev; document Auth0 / Okta config in `docs/runbooks/sso.md`. |
| **Pa11y / Lighthouse CI** may flag legitimate-but-noisy violations on the dev login banner. | Gate threshold at "moderate+ impact" only, with an explicit allowlist file checked into the repo and audited each sprint. |

---

## Out of Sprint 6 (Sprint 7 candidates)

- Federated learning coordinator (Flower) + DP-SGD on the gradient-sharing path.
- TabDDPM-with-DP synthetic-data generation for cold-start.
- Heterogeneous GNN training on the KG.
- SAML SSO if a pilot firm requests it.
- Layer 2 Logic engine (Datalog / Z3).
- Multi-tenant onboarding flow (currently dev-tenant only).
- `case_judges` / `case_courts` edge population (waits on operator-created
  `cases` rows being linkable to the public corpus — S5.6 deferral).

---

*Scoped 2026-05-13 against the Sprint-5 close. Sprint-5 P0 hit 11/13;
S5.1 / S5.2 carry forward as S6.1 / S6.2 with the v1-on-partial-corpus
framing the Sprint-5 risk plan pre-authorised.*
