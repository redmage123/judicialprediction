# Sprint 5 — Real-Data Model + Reasoning Layer 0/2 + Production Hardening

**Cycle:** 2026-07-02 → 2026-07-16 (2 weeks; tentative)
**Issues:** JP-68 onwards (target ~13 stories)

---

## Goal

Sprint 4 shipped a fully-working **demo vertical slice** with persistence,
case list, PDF memo, real SSO, audit log, RBAC, and a re-prediction
history. The demo runs against a **synthetic-data ML model** trained in
S2.13 — meaningful for showing the math, but not legally credible.

Sprint 5 closes that gap: train on real CourtListener data accumulated
during Sprint 4, start the **knowledge graph (Layer 0)** and **NLP
feature extraction (Layer 2)** that the spec calls for, finish the
**HTTP→gRPC** transition deferred from S4.13, and pay down the highest-
value Sprint-3/4 follow-ups (cost-engine, settlement model, real
password reset).

## Demo definition of done

1. ml-inference-svc serves a model trained on real CourtListener tax-court
   opinions (≥500 rows by mid-sprint). MODEL_CARD.md committed with held-out
   metrics + calibration coverage.
2. **Knowledge graph** (Layer 0) has first nodes for judges, courts, and
   cases — populated from `case_documents` + a `judges` seed.
3. **Layer 2 NLP feature extraction** runs over `case_documents` plain_text
   to extract candidate Tier-A/B features (judge severity proxy, case-type
   classification). Operator-typed feature inputs become a fallback, not
   the primary path.
4. api-gateway → ml-inference-svc is now **gRPC** (S4.13 carryover).
5. Real password reset flow (Sprint-4 stub goes away).
6. cost-engine has a real implementation (currently $50k placeholder in
   decision-arith); settlement anchor tied to actual jurisdiction.

## Why this scope

Three tracks:
- **Real data + ML** — closes the "synthetic model" caveat in the demo.
- **Reasoning layers** — first crack at Layer 0 + 2; spec calls for four,
  Sprint 5 lays the foundation.
- **Production hardening** — gRPC transition + real password reset + real
  cost model + settlement anchor. Lifts the demo from "looks ready" to
  "actually ready" for a pilot firm.

---

## Stories

### P0 — Real-data model

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.1**  | Train gradient-boosted ensemble on real tax-court features. Replace ml-inference-svc artefact. MODEL_CARD.md with held-out + calibration metrics. | dev-ai           | S3.6 daily ingest ≥ 500 rows |
| **S5.2**  | Conformal CI calibrated on held-out tax split (target 90% ± 2%). Coverage report in MODEL_CARD. | dev-ai           | S5.1      |

### P0 — gRPC transition (S4.13 carryover, split)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.3**  | Python gRPC server for ml-inference-svc (alongside existing FastAPI HTTP). Add grpc_server.py, regen stubs, 3 tests. | dev-ai           | —         |
| **S5.4**  | Rust tonic client in api-gateway (replaces reqwest HTTP). Env var rename `ML_INFERENCE_URL` → `ML_INFERENCE_GRPC_URL`. | gigforge-engineer | S5.3      |

### P1 — Reasoning Layer 0 (knowledge graph foundation)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.5**  | KG schema migration: nodes (judges, courts, cases), edges (heard_by, in_court, cites). Postgres native (no Neo4j yet — Sprint 7+ if scale demands). | gigforge-engineer | —         |
| **S5.6**  | Populate KG from real `case_documents`: extract judge names, link cases→courts, build `cites` edges from CourtListener `cites` arrays. | dev-ai           | S5.5, S3.6 ingest |

### P1 — Reasoning Layer 2 (NLP feature extraction)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.7**  | NLP pipeline that extracts Tier-A/B feature candidates from `case_documents.full_text_plain` — judge severity proxy via prior-decisions stats, case-type via simple regex. Document accuracy on a small labelled sample. | dev-ai           | S5.6 KG   |
| **S5.8**  | Wire NLP feature output as the default for `createCase`; operator-typed values become an override. UI: input fields prefilled when extraction succeeds. | dev-frontend     | S5.7      |

### P1 — Production hardening

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.9**  | Real password reset flow (replaces S4.8 "contact your admin" stub). Email-link reset via `core.mail` + 1h-ttl token. | dev-backend      | S4.8      |
| **S5.10** | cost-engine real implementation — currently a stub crate. Returns expected litigation cost given case complexity + jurisdiction. | gigforge-engineer | —         |
| **S5.11** | Replace decision-arith's 0.40 settlement anchor with a real settlement model — Nash-Rubinstein bargaining via `decision-arith::settle_offer`. | gigforge-engineer | S5.10     |

### P2 — Quality + cleanup (could-have)

| #         | Story                                                              | Owner            | Deps      |
|-----------|--------------------------------------------------------------------|------------------|-----------|
| **S5.12** | Decision-arith boundary-equality tests + monte-carlo-sim splitmix64 known-vector test (S3.12 follow-ups). | gigforge-engineer | S3.12     |
| **S5.13** | Delete `web/lib/recommend.ts` (S4.4 deprecated); migrate any remaining surfaces to server-computed recommendation. | dev-frontend     | S4.4      |

**13 stories.** P0 (4) + P1 (7) is the realistic landing target; P2 (2) slips to Sprint 6 if needed.

---

## What's deliberately NOT in Sprint 5

- **Layer 3 Logic, Layer 4 Decision-action enrichment** — Sprint 6+. Layer 0 + 2 are enough to make the demo "data-driven, not operator-typed".
- **Real SSO (SAML / OIDC)** — Sprint 6. Email+password from S4.8 is sufficient for early pilots.
- **Federated learning + DP** — Sprint 7+. Multi-firm data sharing belongs after multi-firm onboarding.
- **Partner API (Clio / MyCase)** — Sprint 8+.
- **K8s / multi-region** — out of scope (Hetzner + Compose remains the deploy target).

---

## Risks

| Risk                                                                 | Mitigation                                                                                       |
|----------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| **S5.1 may not have ≥500 rows** if CourtListener daily cron continues hitting the rolling-24h cap. | Track daily delta in `/var/log/jp-courtlistener-daily.log`; if at 5 days in we're below 500 rows, train on what we have + flag in MODEL_CARD as "v1, retrained when more data accumulates". S4.10 (FLP allowlist email) accelerates this. |
| **S5.7 NLP accuracy unknown** — judge-severity proxy from prior decisions is heuristic. | Compare against a 50-case labelled sample during dispatch; if accuracy < 0.7, defer Layer 2 to Sprint 6 and keep operator-typed inputs as primary. |
| **S5.4 gRPC client requires server to be live** during integration test. | Use `tonic-mock` or run the Python server in a subprocess for the e2e test, mirroring the wiremock pattern from S3.1's predict_mutation_happy_path. |
| **S5.10 cost-engine** spec is fuzzy — what counts as "case complexity"? | Sprint 5 ships a *first-pass* cost model: jurisdiction-base × motion-count multiplier, document the simplification; Sprint 6 layers in expected duration + party-count factors. |

---

## Out of Sprint 5 (Sprint 6 candidates)

- Layer 3 Logic layer (rule-application from statutory corpus).
- Layer 4 enrichment (settle/try/borderline → confidence band + counter-recommendation).
- SAML / OIDC SSO.
- A11y CI gate widening (S3.13 follow-up — Pa11y, Lighthouse, all-impact threshold).
- Cargo-mutants weekly cron real Slack channel + first regression alert.

---

*Scoped 2026-05-10 against the Sprint-4 close. JP-67 cancelled in Sprint 4
carries forward as S5.3 + S5.4. Sprint-4 follow-ups (cost-engine,
settlement anchor, password reset, recommend.ts deletion) absorbed into
Sprint 5 as P1/P2 stories.*
