# ADR-003: Multi-tenant isolation strategy

**Status:** Accepted
**Date:** 2026-05-07
**Author:** PM-authored from spec §10 (Multi-Tenancy) + §5 (Compliance Framework) + §9 (Federated Learning + DP)
**Reviewers:** gigforge-engineer (data plane + type system), gigforge-legal (compliance/policy), gigforge-devops (cluster/secrets/operator selection)
**Spec references:** §10 Multi-Tenancy, §5 Demographic / Personality / Compliance Framework, §9 Federated Learning & Differential Privacy
**Plane issue:** JP-13 (Compliance — Feature-store + Proxy Audit + Lineage + Disparate-Impact Reports)

## Context

JudicialPredict serves multiple law firms simultaneously. Each firm's case files contain attorney work product, client communications, and litigation strategy — material that, if leaked across tenant boundaries, ends the company. The legal industry expects:

- **Hard tenant isolation** with cryptographic guarantees, not just role-based access.
- **Auditable access control** — every read of tenant data is logged with subject + reason.
- **Data residency control** — some firms (regulated industries, government contracts) need namespace-per-tenant or even cluster-per-tenant.
- **Right to deletion** — full purge on contract termination, including derived embeddings and ML model gradients.
- **Federated learning compatibility** — tenants can opt into the shared model with formal DP guarantees, without their case data ever leaving their tenant boundary.

We need a strategy that scales from a single pilot firm to enterprise law-firm tenants without architectural rewrites.

## Decision

**Default: shared cluster + shared services + per-tenant data isolation enforced at four layers.** Namespace-per-tenant available for regulated tenants without architectural changes.

### Layer 1 — Database (Postgres row-level security + per-tenant pgvector namespaces)

- Every table containing tenant data carries a `tenant_id` column with a NOT NULL constraint and a foreign key to the `tenants` table.
- **Postgres Row-Level Security (RLS)** policies enforce `tenant_id = current_setting('app.current_tenant_id')::uuid` on every table.
- Application code sets `app.current_tenant_id` at the start of every transaction via the gRPC request metadata. The Rust feature-store maintains this invariant; PRs touching DB access without setting `app.current_tenant_id` are blocked by code review.
- **pgvector namespaces** — each tenant has its own collection name suffix; cross-tenant similarity search is impossible by construction.
- **Per-tenant Postgres roles** with no superuser equivalent; the root role is sealed.

### Layer 2 — Object storage (per-tenant encryption keys)

- MinIO buckets follow `tenant-<uuid>-<purpose>` naming.
- **Per-tenant encryption keys** in AWS KMS (or self-hosted Vault).
- Server-side encryption with KMS keys; key access controlled by IAM-Roles-for-Service-Accounts (IRSA) — only services running with the tenant's role context can decrypt.
- Cross-tenant copy operations require explicit operator authorization with audit log.

### Layer 3 — Compute (gRPC metadata + Rust feature-store enforcement)

- Every gRPC call across the planes carries `tenant-id` and `subject-id` metadata headers.
- The Rust feature-store rejects any call that lacks the metadata or where the metadata doesn't match the JWT bearer's authorized tenant.
- The `Tier`, `Sensitivity`, `PermittedUse` ADTs (per ADR-FP-001 Tier-1 + ADR-004) carry tenant context; cross-tenant feature reads are compile-time impossible without an explicit `cross_tenant_authorized` token type that only the platform admin can produce.
- Per-tenant ML model versions (when applicable in Phase 2) live in MLflow with tenant-prefixed names.

### Layer 4 — Network (NetworkPolicies + per-tenant namespaces for regulated tenants)

- Default: shared K8s namespace `judicialpredict-prod`. NetworkPolicies prevent inter-pod traffic except along approved gRPC paths.
- **Namespace-per-tenant** as opt-in for regulated tenants. Same workloads deployed into `tenant-<uuid>` namespace; NetworkPolicy walls traffic to that namespace only; metrics + logs tagged with tenant ID at the ingestion point.
- **Cluster-per-tenant** as Phase 2 option for the highest-compliance tier (government, healthcare-regulated firms). Same Helm charts, separate ArgoCD Applications, separate KMS keys.

### Federated learning + DP (per spec §9)

- Tenants opt in or out per-tenant. Default opt-out.
- When opted in, only model gradients (not raw data) leave the tenant boundary.
- Gradients pass through Opacus DP-SGD; (ε, δ)-differential-privacy guarantees published per-tenant.
- The Rust secure-aggregation transport encrypts gradients in transit; the coordinator sees only the aggregated update, not individual contributions.
- Per-tenant DP budget (privacy accountant) tracked by the coordinator; tenants can see remaining budget.

### Tenant deletion

- Termination triggers the **tenant-purge job** in Argo Workflows:
  1. Stop all gRPC calls into tenant data (5-minute drain).
  2. Drop tenant rows from every relational table (cascades enforce FK cleanup).
  3. Remove tenant pgvector namespaces.
  4. Delete tenant MinIO buckets.
  5. Revoke tenant KMS keys.
  6. Drop tenant gradients from federated-learning training set (privacy accountant adjusts).
  7. Generate a deletion certificate (signed Markdown record) for the tenant's audit trail.
- Purge is irreversible by design. Pre-deletion snapshot offered as opt-in for the tenant's own retention.

### Audit + observability

- Every tenant-data read logged with `(timestamp, subject, tenant_id, table, row_pk, reason_code)`.
- Per-tenant access reports available to firm admins via Django admin.
- OpenTelemetry traces tagged with `tenant-id` so cross-plane debugging respects boundaries.
- Tenant-aware Grafana dashboards (firm admins see only their tenant's metrics).

## Consequences

### Positive

- **Hard isolation by default.** RLS + per-tenant keys + per-tenant namespaces (where opted) make cross-tenant leakage architecturally impossible, not just policy-controlled.
- **One codebase, multiple tenants.** No fork-per-customer mess. Same services, different tenant context.
- **Compliance-ready posture.** SOC 2 readiness baked in; namespace-per-tenant satisfies regulated-tenant procurement; cluster-per-tenant available without rewrite.
- **FL participation honest.** Tenants who opt into the shared model gain insights without exposing their data; tenants who opt out are not penalized.
- **Tenant lifecycle is automated.** Provisioning + termination both run through Argo Workflows; manual ops is the exception.

### Negative

- **RLS overhead on every query** — Postgres planner penalty ~5-15% on complex joins. Mitigated by query plan review + indexed `tenant_id` on every table.
- **Operational complexity** of per-tenant KMS keys: rotation, backup, recovery scenarios. Mitigated by AWS KMS automatic rotation policies + documented runbook for emergency key recovery.
- **Cross-tenant analytics requires explicit pseudo-anonymization** — disparate-impact reports across tenants need a separate "research namespace" with k-anonymity protection. Phase 2 work; out of Phase 1 scope.
- **Namespace-per-tenant operational cost** — ArgoCD Application count grows linearly with tenant count if every tenant opts into namespace isolation. Mitigated by per-tenant deployments staying within the same cluster (no infrastructure cost multiplier).

### Neutral / mitigations

- **Phase-2 right-of-refusal:** the cluster-per-tenant pathway exists architecturally but is only triggered by specific high-compliance tenants. We don't pay that cost until a tenant requires it.
- **Backwards compatibility:** RLS rules are versioned with the schema; legacy data backfilled with `tenant_id` during migration.

## Alternatives considered

### Alternative A — Single tenant per database / cluster
**Rejected** as the default. Operational cost grows linearly with tenant count: per-tenant Postgres clusters, per-tenant Neo4j, per-tenant Redis, per-tenant ArgoCD apps. Costs out the floor at low scale; the marginal compliance gain over RLS+namespace+KMS is not worth the cost. Available as opt-in for regulated tenants (Layer 4 cluster-per-tenant Phase 2).

### Alternative B — Application-level filtering (no RLS, just `WHERE tenant_id = ?` everywhere)
**Rejected.** The first time an engineer forgets the `WHERE` clause is a Title VII discovery in the next breach notification. RLS makes the isolation a property of the database, not of the application code.

### Alternative C — Schema-per-tenant
**Considered.** Postgres schemas per tenant give isolation similar to RLS but at the cost of (a) schema migration becoming a per-tenant operation that must run for every tenant on every release, (b) cross-schema joins becoming architecturally awkward when we need pseudo-anonymized analytics, (c) connection-pool overhead growing with tenant count. RLS gives 80% of the benefit at 20% of the cost.

### Alternative D — Encrypted-at-rest with no tenant separation
**Rejected.** Encryption is necessary but not sufficient; it doesn't prevent in-flight cross-tenant reads in the application layer. Defense-in-depth requires both.

## Compliance and verification

- **Property tests:** `feature-store-types` crate property-tests assert that no `Tier::C` value can be read across tenants without an explicit `cross_tenant_authorized` token. Compile-time + property-time enforcement.
- **Integration tests:** `tests/multi-tenant/` runs cross-tenant read attempts in CI; any successful cross-tenant data access fails the test.
- **CI gate:** any new table without a `tenant_id` column + RLS policy is rejected by a CI lint that scans migration files.
- **Quarterly security audit:** external review of tenant isolation; audit findings tracked as P0 backlog items.
- **Pen test pre-pilot:** specifically targets cross-tenant boundary violations; pilot launch is gated on clean pen-test results.
- **Continuous monitoring:** anomalous cross-tenant access patterns trigger PagerDuty alerts.

## References

- `judicialpredict-v2-spec.md` §10 (Multi-Tenancy)
- `judicialpredict-v2-spec.md` §5 (Demographic / Personality / Compliance Framework)
- `judicialpredict-v2-spec.md` §9 (Federated Learning + Differential Privacy)
- `judicialpredict-v2-spec.md` §11.5 (Platform — Kubernetes + GitOps)
- ADR-001 (Polyglot architecture boundary)
- ADR-002 (gRPC contracts as single source of truth)
- ADR-FP-001 (Functional-core / imperative-shell paradigm boundaries)
- ADR-004 (Compliance feature-tier enforcement at type-system boundary) — to be authored next
- Postgres RLS documentation: https://www.postgresql.org/docs/current/ddl-rowsecurity.html
- AWS KMS best practices: https://docs.aws.amazon.com/kms/latest/developerguide/best-practices.html

---

*This ADR is part of the JudicialPredict architectural decision record. ADRs are append-only; supersession is documented via `Superseded by` not by edit.*

---

## Engineer Review — 2026-05-07

**Reviewed by:** gigforge-engineer (Chris Novak persona, Claude Sonnet 4.6)
**Code artifacts inspected:**
- `rust/feature-store/migrations/20260507120000_baseline.sql`
- `rust/feature-store/migrations/20260507120001_seed_dev_tenant.sql`
- `rust/feature-store/migrations/20260507120002_app_role.sql`
- `rust/feature-store/migrations/20260507120003_jp_app_password.sql`
- `rust/feature-store/src/lib.rs` — Repo + `set_tenant_context`
- `rust/api-gateway/src/app.rs` — JWT middleware extracting tenant_id
- `rust/api-gateway/src/auth.rs` — `Claims` struct + `decode_jwt`
- `rust/api-gateway/tests/e2e_smoke.rs` — cross-tenant RLS smoke test
- Live RLS smoke test output (verified 2026-05-07)

### Aspects matching shipped reality

- **Every table has `tenant_id` NOT NULL + FK to `tenants`:** ✅ `cases`, `features` both have `tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE`. Index on `tenant_id` on every tenant-scoped table, plus composite `(tenant_id, case_id)` on `features` and `(tenant_id, judge_name)` on `cases`.
- **RLS policy `tenant_id = current_setting('app.current_tenant_id', true)::uuid` on every table:** ✅ `cases`, `features`, and `tenants` all have `CREATE POLICY tenant_isolation ... USING (tenant_id = ...) WITH CHECK (tenant_id = ...)`.
- **`FORCE ROW LEVEL SECURITY` on all tenant-scoped tables:** ✅ Added in the baseline migration. This was a critical gap identified during smoke testing — without `FORCE`, the migration superuser (which has `Bypass RLS` privilege) bypasses all policies. `FORCE` ensures the constraint holds for the table owner too.
- **`app.current_tenant_id` set at transaction start:** ✅ `rust/feature-store/src/lib.rs` implements `set_tenant_context(pool, tenant_id)` which executes `SET LOCAL app.current_tenant_id = '<uuid>'` before every query.
- **`jp_app` non-superuser role for application runtime:** ✅ Migration `20260507120002_app_role.sql` creates `jp_app` as a non-superuser with `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES` and `REVOKE SUPERUSER`. This is the role the application connects as; without it, RLS is bypassed by the owner.
- **JWT carries tenant_id claim:** ✅ `Claims` struct in `auth.rs` has `tenant_id: Uuid` extracted from the JWT; the middleware stores it in Axum request extensions; every gRPC metadata header to feature-store carries it.
- **Cross-tenant blocked by RLS — e2e verified:** ✅ `graphql_feature_rls_smoke` test: feature inserted for tenant A; query with tenant B JWT returns null (blocked). RLS smoke test from migration step: cross-tenant INSERT raises "new row violates row-level security policy".
- **`audit_log` table with `(tenant_id, subject_id, table_name, row_pk, action, reason_code, ts)`:** ✅ Schema exactly as specified. No RLS on `audit_log` (append-only; service-layer writes; DBA reads).

### Divergences from seed

1. **Key finding not in ADR: superuser bypasses RLS even with FORCE.** The ADR says "per-tenant Postgres roles with no superuser equivalent; the root role is sealed." This is the right intent but the mechanism was not fully specified: the application must never connect as a superuser. In practice, the migration role (`judicialpredict`) is a superuser (created by the docker-compose setup), which bypasses RLS regardless of `FORCE`. The fix — the `jp_app` non-superuser role in migration 0002 — is correct but was not anticipated in the seed.

   **Amendment added below:** The ADR should note that the _application runtime connection role_ must be a non-superuser, and that `FORCE ROW LEVEL SECURITY` alone is insufficient if the connecting role has superuser privileges.

2. **pgvector namespace isolation not yet implemented.** The ADR specifies per-tenant pgvector namespace (collection name suffix). The baseline migration enables the `vector` extension but no embedding column or per-tenant namespace exists yet. The `features.value` column is `jsonb`, not `vector(N)`. **Sprint 2 gap** — the vector embedding column and namespace enforcement are Phase 2 work per the spec.

3. **Object storage (MinIO per-tenant KMS keys) not yet implemented.** Layer 2 (per-tenant encryption keys, per-tenant buckets) is scaffolded in `docker-compose.dev.yml` (MinIO service present) but not wired at the application level. No bucket-creation logic, no KMS key assignment. **Phase 2 gap.**

4. **Tenant deletion purge job not yet built.** The Argo Workflows purge job is spec'd but not implemented. **Phase 2 gap.**

5. **`tests/multi-tenant/` integration test suite not yet created.** The ADR mandates a CI test that runs cross-tenant read attempts and fails on any successful cross-tenant data access. The e2e smoke test in `api-gateway/tests/e2e_smoke.rs` covers the happy path and the cross-tenant blocked path for the api-gateway → feature-store vertical slice, but a dedicated `tests/multi-tenant/` suite does not exist. **Sprint 2 gap.**

### Amendment — 2026-05-07

**Addendum to Layer 1 — Database:**

> **Critical operational note:** `FORCE ROW LEVEL SECURITY` alone is insufficient when the connecting database role has superuser privileges. Postgres superusers bypass RLS regardless of `FORCE`. The application must always connect as the non-superuser `jp_app` role (or equivalent). The superuser migration role must never be used in application runtime connection strings. Migration files that run as the superuser intentionally bypass RLS to seed data — this is correct for migration scripts only.

> **Role convention:** migration scripts run as the superuser `judicialpredict`; application runtime uses `jp_app` (non-superuser, no `CREATEROLE`, no `BYPASSRLS`, no `SUPERUSER`). This is enforced by `DATABASE_URL` in application config pointing to `jp_app`; the superuser credential is ops-only.
