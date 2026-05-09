"""
Audit hook for tenant_settings override changes — Option B (Sprint 3).

Writes one ``audit_log`` row per changed override key directly from Django.
This mirrors the diff-and-record logic implemented in the Rust
``update_overrides()`` function (rust/feature-store/src/tenant_settings.rs).

Sprint-4 follow-up (JP-53): consolidate on the gRPC UpdateTenantSettings
client (Option A).  When that lands, replace ``record_override_changes()``
with a call to ``core.feature_store_client.FeatureStoreClient.update_tenant_settings()``,
which delegates diff + audit to the Rust source-of-truth and eliminates
the duplicate diff logic here.

audit_log schema (from 20260507120000_baseline.sql +
20260507120004_audit_log_rls_and_outbound_cols.sql):
    id          bigserial PRIMARY KEY
    tenant_id   uuid
    subject_id  text          -- operator email (actor)
    table_name  text NOT NULL
    row_pk      text          -- stringified tenant_id
    action      text NOT NULL -- 'tenant_settings.override_change'
    reason_code text          -- SHA-256 hex of the new overrides JSON
    ts          timestamptz   DEFAULT now()
    latency_ms  integer
    cost_micros integer

Grant situation:
    jp_app   (default alias) — INSERT on audit_log ✓  (migration 2)
    jp_admin (admin_super alias) — INSERT on audit_log ✓  (migration 9)
The correct connection is selected via ``get_current_db_alias()`` from the
RLS middleware thread-local, which has already been set for this request.
"""

from __future__ import annotations

import hashlib
import json

from django.db import connections

from core.middleware import get_current_db_alias

AUDIT_ACTION = "tenant_settings.override_change"


# ---------------------------------------------------------------------------
# Pure diff helper — mirrors Rust diff_overrides()
# ---------------------------------------------------------------------------


def diff_overrides(old: dict, new: dict) -> list[str]:
    """
    Return a list of feature names that changed between *old* and *new*
    override dicts.  Returns an empty list when the dicts are identical.

    Mirrors ``diff_overrides()`` in tenant_settings.rs:
    - ``disabled_features``: symmetric difference of the two sets.
    - ``tier_overrides``: keys that were added, removed, or changed value.
    """
    changed: set[str] = set()

    old_disabled: set[str] = set(old.get("disabled_features", []))
    new_disabled: set[str] = set(new.get("disabled_features", []))
    changed |= old_disabled.symmetric_difference(new_disabled)

    old_tier: dict[str, str] = old.get("tier_overrides", {})
    new_tier: dict[str, str] = new.get("tier_overrides", {})
    all_keys: set[str] = set(old_tier) | set(new_tier)
    for key in all_keys:
        if old_tier.get(key) != new_tier.get(key):
            changed.add(key)

    return list(changed)


# ---------------------------------------------------------------------------
# Audit write
# ---------------------------------------------------------------------------


def record_override_changes(
    tenant_id: object,
    old_overrides: dict,
    new_overrides: dict,
    actor_email: str,
) -> int:
    """
    Diff *old_overrides* against *new_overrides* and write one ``audit_log``
    row per changed key.

    Returns the number of rows written (0 = idempotent no-op).

    ``payload_hash`` (stored in ``reason_code``) is the SHA-256 hex digest of
    the canonicalised new overrides JSON (sort_keys=True, lists sorted).
    All rows from a single save share the same hash — consistent with the
    Rust ``hash_payload(new_json.to_string())`` approach.

    Uses the DB alias currently active for this request thread
    (``get_current_db_alias()``), so jp_app and jp_admin connections are
    both handled correctly.
    """
    changed_keys = diff_overrides(old_overrides, new_overrides)
    if not changed_keys:
        return 0

    # Canonicalise for deterministic hashing: sort dict keys AND list contents.
    canonical = {
        "disabled_features": sorted(new_overrides.get("disabled_features", [])),
        "tier_overrides": dict(
            sorted(new_overrides.get("tier_overrides", {}).items())
        ),
    }
    payload_hash = hashlib.sha256(
        json.dumps(canonical, sort_keys=True).encode()
    ).hexdigest()

    db_alias = get_current_db_alias()
    with connections[db_alias].cursor() as cursor:
        for _ in changed_keys:
            cursor.execute(
                """
                INSERT INTO audit_log
                    (tenant_id, subject_id, table_name, row_pk, action, reason_code)
                VALUES (%s, %s, %s, %s, %s, %s)
                """,
                [
                    str(tenant_id),
                    actor_email,
                    "tenant_settings",
                    str(tenant_id),
                    AUDIT_ACTION,
                    payload_hash,
                ],
            )

    return len(changed_keys)
