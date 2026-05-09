# Tenant Settings — Per-Tenant Feature-Tier Override Store

**Sprint:** S2.12 · **Plane ticket:** JP-35
**ADRs:** ADR-003 (multi-tenant isolation), ADR-FP-001 (functional-core/imperative-shell)

---

## Overview

The `tenant_settings` table holds per-tenant override configuration for the
feature-store.  It lets an operator **tighten** the global tier policy for a
specific firm — for example, permanently disabling a Tier-B feature or
downgrading it to Tier-C — without touching the global feature definitions.

> **Tightening only.**  Overrides cannot grant a feature that the global tier
> policy forbids.  A tenant that is globally allowed Tier-B features can
> restrict itself further; it cannot unlock Tier-C features.

---

## Database Schema

```sql
CREATE TABLE tenant_settings (
    id                     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id              uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE UNIQUE,
    feature_tier_overrides jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now()
);
```

Migration file: `rust/feature-store/migrations/20260509120000_tenant_settings.sql`

RLS policies follow the same pattern as `features` and `cases`:

| Policy                    | Operation | Condition                                         |
|---------------------------|-----------|---------------------------------------------------|
| `tenant_settings_select`  | SELECT    | `tenant_id::text = current_setting('app.current_tenant_id')` |
| `tenant_settings_insert`  | INSERT    | same                                              |
| `tenant_settings_update`  | UPDATE    | same (USING + WITH CHECK)                         |

`jp_app` is granted `SELECT, INSERT, UPDATE` — no DELETE (immutable audit trail principle).

---

## jsonb Shape

```json
{
  "disabled_features": [
    "attorney_personality_score",
    "judge_age_years"
  ],
  "tier_overrides": {
    "attorney_temperament": "TIER_C"
  }
}
```

### `disabled_features`

An array of stable feature names.  Any feature whose name appears here is
refused with `gRPC PERMISSION_DENIED` regardless of its assigned tier.  Use
this for features that are globally Tier-A but a firm has contractually agreed
not to use.

### `tier_overrides`

A map of `feature_name → "TIER_A" | "TIER_B" | "TIER_C"`.  Only downgrading
is meaningful:

| Override value | Effect                                                   |
|----------------|----------------------------------------------------------|
| `TIER_A`       | No enforcement change (feature remains Tier-A or better) |
| `TIER_B`       | No enforcement change                                    |
| `TIER_C`       | Feature is refused; handler returns `PERMISSION_DENIED`  |

Setting `tier_overrides["X"] = "TIER_C"` is functionally equivalent to adding
`"X"` to `disabled_features` but is semantically richer: it signals *why* the
feature is refused (protected-class tier).

---

## Rust Types

```rust
pub struct TenantOverrides {
    pub disabled_features: HashSet<String>,
    pub tier_overrides:    HashMap<String, FeatureTier>,
}

pub enum FeatureTier { TierA, TierB, TierC }

/// Pure enforcement function — no I/O.
pub fn check_feature_allowed(
    overrides: &TenantOverrides,
    feature_name: &str,
) -> Option<String>  // Some(reason) → PERMISSION_DENIED
```

---

## In-Process Cache

`OverridesCache` wraps `Arc<DashMap<Uuid, CacheEntry>>` with a 60-second TTL.

```
┌─────────────────────────────────────────────────────────┐
│  gRPC handler                                           │
│   → get_overrides(pool, tenant_id, &cache)              │
│       ├─ cache hit  (< 60 s)  → return immediately      │
│       └─ cache miss           → SELECT tenant_settings  │
│                                   → cache.set()         │
└─────────────────────────────────────────────────────────┘
```

Cache is invalidated immediately after `update_overrides` completes so the next
read reflects the new data.  There is a small window (< 60 s on other replicas)
where stale overrides could be served; this is acceptable for Sprint 2 and will
be eliminated by the Redis-backed store in Sprint 3.

---

## Admin Update Path

```
POST http://feature-store-host:4002/admin/tenant-settings
Authorization: Bearer <ADMIN_TOKEN>
Content-Type: application/json

{
  "tenant_id": "00000000-0000-0000-0000-000000000001",
  "overrides": {
    "disabled_features": ["attorney_personality_score"],
    "tier_overrides": {"judge_age_years": "TIER_C"}
  }
}
```

The handler:
1. Loads current overrides (diff baseline).
2. UPSERTs new overrides into `tenant_settings`.
3. Invalidates the in-process cache.
4. Writes one `audit-recorder` event per added/removed/changed override key:
   - `action = "tenant_settings.override_change"`
   - `payload_hash = SHA-256(new_jsonb_string)`
   - `cost_micros = None`

**Sprint-3 follow-up:** the static `ADMIN_TOKEN` check must be replaced with
proper JWT validation backed by the operator RBAC system (JP-38).

---

## gRPC Enforcement

`GetFeature` and `ListFeatures` call `get_overrides` + `check_feature_allowed`
after the DB fetch:

```
GetFeature(request)
  └─ set_tenant_context + get_feature (DB)
  └─ get_overrides(cache)              ← 60-s TTL cache
  └─ check_feature_allowed(name)
      ├─ None   → return Feature
      └─ Some(reason) → Status::permission_denied(reason)
```

`ListFeatures` emits a `PERMISSION_DENIED` stream item for the first blocked
feature and terminates the stream (not a silent drop per spec §3).

---

## Swap-to-Redis Path (Sprint 3)

The `OverridesCache` has a clean seam for a Redis replacement:

1. Implement a Redis-backed store (e.g. `RedisOverridesCache`) that
   `GET`/`SETEX`-es a JSON blob per tenant.
2. Replace `Arc<DashMap<...>>` with the Redis client in `OverridesCache`.
3. `invalidate()` calls `DEL` instead of `remove()`.
4. TTL is managed by Redis `SETEX` (60 s) rather than `Instant::elapsed()`.

The `get_overrides` / `update_overrides` function signatures are unchanged;
only the `OverridesCache` internals change.  The gRPC handlers and admin
endpoint require no modification.

This also eliminates the stale-override window on horizontally-scaled
feature-store replicas.

---

## Sprint-3 Follow-ups

| Item | Ticket |
|------|--------|
| Django admin UI for `tenant_settings` | JP-38 |
| Operator RBAC: replace static `ADMIN_TOKEN` with JWT | JP-38 |
| Redis-backed `OverridesCache` (eliminate cross-replica staleness) | Sprint 3 |
| `ListFeatures` sliding-window fix: return all permitted features, error only for blocked ones | Sprint 3 |
| "Loosening" guard: reject override requests that attempt to grant forbidden tiers | Sprint 3 |
