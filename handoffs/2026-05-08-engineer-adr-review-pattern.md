# Handoff — ADR Engineer Review + ADR-005

**From:** gigforge-engineer (Chris Novak)
**To:** gigforge-pm (Jamie Okafor), gigforge-qa (Riley Svensson)
**Date:** 2026-05-07
**Plane issue:** JP-2

---

## Summary

All four PM-seeded Sprint 1 ADRs have been engineer-reviewed. ADR-005 has been authored. The retrospective gate for Sprint 1 ADR reviews is now clear.

---

## ADR Reviews

### ADR-002 — gRPC contracts as single source of truth

**Verdict: Accepted as-is (no design amendments)**

Core decision matches reality exactly: `protos/` as canonical location, buf lint + buf breaking in CI, prost+tonic on Rust, buf.gen.yaml for Python codegen.

**Execution gaps (Sprint 2 backlog):**
- `buf generate` not wired into CI (Python stub compilation not validated on every proto change)
- Generated Python stubs not committed + stamp-checked in CI
- Cross-plane integration test (matched Rust + Python images) not yet built
- `CODEOWNERS` for `protos/` not yet created

---

### ADR-FP-001 — Functional core, imperative shell

**Verdict: Accepted as-is (no design amendments)**

All four Rust Tier-1 crates (`decision-arith`, `monte-carlo-sim`, `cost-engine`, `feature-store-types`) have the `// FUNCTIONAL-CORE` marker, zero mutable global state, no unsafe, and proptest suites covering the specified algebraic invariants.

One implementation note: `monte-carlo-sim` required a `splitmix64` hash to replace the original LCG that produced non-uniform output for consecutive seeds — caught by the convergence proptest. Fix is correct and consistent with the ADR's pure-deterministic-seed intent.

**Execution gaps (Sprint 2 backlog):**
- `static mut` / I/O rejection linter for `// FUNCTIONAL-CORE` files not wired in CI
- Python Tier-1 modules (logic-svc, causal-inference-svc, nlp-svc, ml-inference-svc/conformal) not yet scaffolded
- Proptest count gate not in CI

---

### ADR-003 — Multi-tenant isolation strategy

**Verdict: Accepted with in-place amendment (critical operational note)**

RLS implementation is correct and smoke-tested: `FORCE ROW LEVEL SECURITY` on all three tenant-scoped tables, `(tenant_id = current_setting('app.current_tenant_id', true)::uuid)` on USING + WITH CHECK, `jp_app` non-superuser role for application runtime, cross-tenant blocked confirmed by live test.

**Critical finding (amendment appended to ADR-003):** `FORCE ROW LEVEL SECURITY` alone is insufficient when the connecting role is a Postgres superuser. The migration role (`judicialpredict`) has `Bypass RLS` — superusers bypass RLS unconditionally. Correct fix (shipped): `jp_app` non-superuser role used for application runtime; migration scripts run as superuser intentionally. This was not explicitly stated in the PM seed and is now documented in an amendment.

**Phase 2 gaps:**
- pgvector per-tenant namespaces not yet implemented
- MinIO per-tenant KMS keys not wired
- Tenant purge job not built
- `tests/multi-tenant/` integration suite not yet created

---

### ADR-004 — Compliance tier type-system enforcement

**Verdict: Accepted with in-place amendment (phased implementation plan)**

`Tier`, `Sensitivity`, `PermittedUse` enums and `TieredFeature<T>` wrapper are correctly implemented. Runtime Tier-C gate (`read(permitted_use: Option<PermittedUse>) -> Option<&T>`) works correctly and is unit+property tested.

**Design gap (amendment appended to ADR-004):** The PM seed describes full phantom-type compile-time enforcement (`Feature<TierA, Public>`, `PermittedUseInModel` trait, `TierC` not impl). The shipped Sprint 1 code uses a simpler runtime enum — the compile-time rejection the ADR promises does not yet exist. This is a Sprint 2 target, not a Sprint 1 failure. The amendment documents the two-phase plan: runtime enum (Sprint 1 baseline) → phantom-type system (Sprint 2 target) with a `into_typed()` migration path.

**Sprint 2 gaps:**
- Phantom-type `Feature<TierA|B|C|D, Sensitivity>` not yet implemented
- `ProtectedClassElementToken` and `CrossTenantAuthorizedToken` not yet built
- Feature metadata registry (`permitted_uses`, `provenance` columns) not yet added
- Protected-class proxy audit job is Phase 2

---

## ADR-005 — PM-seed-then-engineer-amend pattern

**Status: Authored, Accepted**
**Path:** `adrs/adr-005-pm-seed-then-engineer-amend-pattern.md`

Formalises the Sprint 1 pattern as a documented process: PM seeds from spec, flags for review, engineer appends review section within one sprint cycle, minor divergences amended in-place, major disagreements trigger a new superseding ADR. Sprint retrospective is gated on all ADR reviews being committed.

---

## Sprint 3 follow-ups

1. Wire `buf generate` + Python stub commit-and-stamp into CI (ADR-002 gap).
2. Build `Feature<TierA|B|C|D, Sensitivity>` phantom-type system; add `ProtectedClassElementToken` (ADR-004 gap).
3. Wire `FUNCTIONAL-CORE` linter for both Rust (clippy custom lint) and Python (ruff plugin) (ADR-FP-001 gap).
4. Create `CODEOWNERS` for `protos/` (ADR-002 gap).
5. Create `tests/multi-tenant/` CI suite (ADR-003 gap).
6. Wire ADR header-stamp CI lint (check that `PM-authored` ADRs have an `## Engineer Review` section) — per ADR-005.
