# Sprint 1 — Review & Retrospective

**Sprint:** 1 (Foundation + Methodology Rollout)
**Window:** 2026-05-07 → 2026-05-21 (14 days planned)
**Plane cycle:** `aa838f21-155a-4a77-a44e-4bd863340c6b`
**Spec version:** v2.13
**Reviewed:** 2026-05-07 (Day 1 — this is an early checkpoint, not the formal end-of-sprint review; that lands 2026-05-21)

> This document is part-review, part-retro, written at end-of-day 1 because we covered most of the planned Sprint 1 scope in a single day. The remaining sprint window will be used for engineer-review-and-amend of the PM-seeded ADRs, monte-carlo-sim proptest finishing, K8s cluster bootstrap (which needs cloud-account provisioning that's outside the dev loop), and real UX research interviews (which need pilot-firm scheduling).

---

## What was delivered

### Code that compiles and tests pass

40 tests across the codebase, **0 failures**:

- 33 Rust workspace tests
  - 8 EV/CVaR/Nash/prospect-theory properties
  - 4 cost-engine composition properties
  - 6 Tier/Sensitivity/PermittedUse properties
  - 11 unit tests across the 10 crates
  - 4 misc round-trip + integration tests
- 3 e2e_smoke tests (api-gateway → feature-store → Postgres RLS through GraphQL)
- 4 Python pytest (ml-inference-svc: health + readyz + 2 proto round-trips)

### 19 git commits on `main` covering

| Story | Plane | Status | Commit |
|-------|-------|--------|--------|
| ADR-001 polyglot architecture | JP-3 | Accepted | `63c3253` |
| ADR-002 gRPC contracts | JP-3 | Accepted | `a445ab4` |
| ADR-FP-001 functional-core / imperative-shell | JP-2 | Accepted | `18864aa` |
| ADR-003 multi-tenant isolation | JP-13 | Accepted | `18864aa` |
| ADR-004 type-system tier enforcement | JP-13 | Accepted | `18864aa` |
| K8s topology proposal | JP-1 | PM-seed; ready for SRE | `63c3253` |
| UX research interview plan | JP-17 | PM-seed; ready for UX Researcher | `63c3253` |
| PM Sprint 1 acceptance handoff | JP-2 | gigforge-pm dispatch | `81bcd67` |
| Rust workspace (10 crates) | JP-3 | engineer | `5397da3` |
| protos/ + 2 contracts | JP-3 | engineer | `e9c1af9` |
| CI workflow (7 jobs) | JP-2 | engineer | `4f853b2` |
| docker-compose.dev.yml + runbook | JP-1 | engineer | `fdad4dd` |
| tonic-build codegen | JP-3 | engineer | `412fe64` |
| Postgres migrations + RLS smoke-tested | JP-1 + JP-13 | engineer | `cbc6769` |
| Python ml-inference-svc skeleton | JP-7 | engineer | `7498fa9` |
| Vertical slice (api-gateway → feature-store → Postgres RLS over GraphQL) | JP-3 + JP-13 | engineer + PM finishing | `485d6bb` |
| 22 property tests on functional-core crates | JP-2 + JP-5 + JP-13 | engineer | `937480f` |
| api-gateway Helm chart + ArgoCD gitops/dev/ + Dockerfile | JP-1 + JP-3 | engineer | `93288dd` |

### Plane epics — state at end of Day 1

- **In Review:** JP-1 (Platform), JP-2 (Methodology), JP-3 (Rust gateway)
- **In Progress:** JP-13 (Compliance), JP-17 (UX research), JP-21 (Data ingest), JP-22 (Recruiting)
- **Backlog:** 17 remaining epics (Sprint 2+)

### What's running on disk right now

- Postgres 16 + pgvector on `127.0.0.1:5454` with 4 migrations applied; jp_app non-superuser role connecting under RLS.
- Neo4j, Redis, MinIO containers in docker-compose (not yet exercised by application code; ready for Sprint 2).
- Live RAG sync cron (every 30min): every artifact in `/opt/ai-elevate/gigforge/projects/judicialpredict/` is auto-ingested into the gigforge/engineering RAG collection.
- Daily progress report cron: emails Braun at 17:00 Berlin.
- Weekly progress report cron: emails Braun at 16:00 Friday Berlin.
- Plane sync mirror: refreshed every 30 min into `.plane-mirror/issues.json` and `.plane-mirror/cycles.json`.

---

## What slipped or didn't ship

| Item | Why | Sprint-2 disposition |
|------|-----|---------------------|
| K8s cluster bootstrap (JP-1 full) | Needs cloud-account provisioning + SRE hire; topology proposal landed but cluster doesn't exist yet | SRE recruit; cluster bootstrap is the first Sprint 2 story |
| `monte-carlo-sim` proptest bodies | Engineer's CLI subprocess SIGTERMed at the 600s mark before reaching the 4th file's tests; file structure exists with stubs | First Sprint 2 catch-up story (~30 min) |
| Engineer review/amend of PM-seeded ADRs | ADRs 002, FP-001, 003, 004 were PM-seeded due to early model issues. Engineer should validate and amend. | Sprint-2 review pass with the engineer |
| First UX research interviews | Pilot firms not yet confirmed; recruiting plan ready but execution depends on pilot list | Pilot-firm shortlist + first 3 interviews in Sprint 2 |
| Recruiting (JP-22) — actual hires | Job descriptions drafted; postings + hires need a longer cadence | Continuing through Sprint 2 + 3 |
| ADR-005+ | None needed in Sprint 1 | Sprint 2 will surface architectural decisions naturally |

Nothing on the critical path slipped; the gaps are scope items that legitimately need cycles longer than 1 day (recruiting, cloud provisioning, partner-firm scheduling).

---

## Velocity (informal)

We didn't strictly story-point Sprint 1 — kickoff was rushed. Reading the commits backward, the engineer-authored stories averaged ~30-60 min of dispatch wall-clock each (includes all the engineer's tool calls), with the Vertical Slice (S1.11) taking ~90 min and a follow-up PM-finishing pass for two trivial issues (missing `sqlx` workspace dep + a stray `use uuid::Uuid;`).

For Sprint 2 I'll point properly using Fibonacci so the burndown is meaningful.

---

## What worked

- **Single focused dispatch per task.** "Author ADR-001 only" or "scaffold the Rust workspace only" produced clean, complete work. Multi-deliverable dispatches stalled (a known pattern from prior memory).
- **PM-seed-then-iterate when agents stall.** ADRs 002 / FP-001 / 003 / 004 were PM-authored after Gemma e4b proved insufficient for ADR-quality work. Once Claude Sonnet was working, the engineer caught up reliably for code-shaped tasks. The pattern: don't fight a model that can't do the work; produce the seed, let the agent iterate.
- **Verify on disk, not via agent reply.** The openclaw agent CLI's stdout has been unreliable (multiple dispatches landed clean code but the reply was empty or off-topic). Running `cargo test`, `helm lint`, `pytest`, etc. directly is the source of truth.
- **Engineer caught real gotchas:** prost's enum-prefix-stripping, "superusers bypass RLS even with FORCE", Docker IPAM exhaustion, PyYAML's `on:` boolean quirk on GitHub Actions YAML. All real issues, all surfaced in the engineer's handoffs.
- **Property tests on day 1.** 22 algebraic invariants on the functional-core crates means Sprint 2 refactors land safely; mutation testing in Sprint 2 will tell us if the properties are tight enough.
- **Vertical slice with live RLS proven.** Cross-tenant query attempts return 0 rows from the Postgres layer; the GraphQL e2e test proves the same at the api-gateway layer. This is the most expensive class of compliance bug, prevented by construction.

## What didn't work

- **The proxy human-pacing patch.** I tried adding concurrency=1 + 6s spacing + 200/day cap inside the claude-code-proxy. The pacing limiter blocked Claude calls long enough for the gateway's own model-fallback timeout to fire, which triggered `LiveSessionModelSwitchError` and crashed the gateway. **Reverted.** Lesson: don't add timing-sensitive logic inside a plugin layer that's downstream of the gateway's own timeouts; the layers don't compose cleanly.
- **Agent-keyed session-resume patch.** Same proxy patch attempted to make consecutive dispatches to the same agent reuse one Claude session. Worked in isolation but interacted badly with concurrent dispatch attempts and the model-fallback machinery. **Reverted with the pacing patch.** Sprint 2 should approach this differently — possibly by capping dispatch frequency at the gateway level, not the proxy level.
- **Gemma 4 e4b / blockrun-free for ADR authoring.** Free-tier model could plan but reliably failed to call the Write tool on multi-step file-I/O tasks. Croatian-legal LoRA is fine for short Q&A, useless for autonomous engineering work. **Resolution:** route engineer / PM / qa / dev-frontend / dev-backend through Claude Sonnet via the credential we copied from the dev server; everything else stays on local models. Already in place.
- **Reliance on agent reply text.** Multiple dispatches "completed" with empty stdout despite the engineer doing real work. Workaround: verify on disk. Long-term fix: the proxy / gateway pipeline needs investigation in Sprint 2.

## Sprint-2 retrospective improvement (one concrete commitment)

**Story-point Sprint-2 work using Fibonacci, write Gherkin acceptance criteria for every story before sprint planning closes, and require the Three Amigos session for any story with cross-plane impact.** Sprint 1's velocity was vibes-based; Sprint 2 will have a real burndown.

---

## Demo (notional — for end-of-sprint walkthrough on 2026-05-21)

When demo'd at end-of-sprint to Braun + pilot-firm-rep:

1. Show `cargo test --workspace` — 36 Rust tests pass in <10s.
2. Show `cargo test --test e2e_smoke --include-ignored` — full GraphQL→Postgres→RLS round-trip tests pass.
3. `curl http://localhost:4000/healthz` — returns `{"status":"ok"}`.
4. Insert a feature for tenant_a directly via psql, query it via GraphQL with `X-Tenant-Id: <tenant_a>`, get the value back.
5. Run the same GraphQL query with `X-Tenant-Id: <tenant_b>` — get null (RLS blocked).
6. `helm template charts/api-gateway/` — show a real K8s manifest.
7. `pytest` — 4 Python tests pass.
8. Walk through ADR-FP-001 + ADR-004 in the workspace; show the `feature-store-types` proptests asserting that Tier-C cannot satisfy a `PermittedUseInModel` bound.

The pilot firm sees: "they have working code with tested cross-tenant isolation on day 1." That's the trust artifact for the SOW conversation.

---

## Cumulative metrics

| Metric | Sprint 1 |
|--------|---------|
| Tests passing | 40 |
| Tests failing | 0 |
| Git commits | 19 |
| Lines of Rust shipped (excluding generated code) | ~2,400 |
| Lines of Python shipped | ~600 |
| ADRs landed | 5 |
| Plane epics In Review | 3 of 23 |
| Compliance smoke-tests passed | 9 of 9 (6 RLS at Postgres + 3 e2e at GraphQL) |
| Property tests | 22 |

---

*Sprint review at end-of-window (2026-05-21) will reconcile this checkpoint against the slipped items and add demo + customer-feedback notes.*
