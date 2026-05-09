"""
Tests for the tenant_settings Django admin UI (S3.10 / JP-51).

REQUIRES POSTGRES: all tests in this module hit the database.
Run against the dev stack:
    cd python/admin && uv run pytest tenant_settings/

The setUp helper creates the unmanaged tables (tenants, tenant_settings,
audit_log) using CREATE TABLE IF NOT EXISTS so the tests are safe to run
against an already-migrated stack.

Test inventory (6 tests):
    1. change_form_shows_all_feature_names     — all 7 features in the UI
    2. save_two_disabled_features              — DB row + 2 audit rows
    3. idempotent_save_writes_zero_audit_rows  — diff = empty → 0 audit rows
    4. tier_upgrade_attempt_is_rejected        — TIER_A → form error, no DB change
    5. unknown_feature_in_disabled_is_rejected — unknown name → form error
    6. viewer_cannot_save                      — role=viewer → POST returns 403
"""

import uuid

from django.contrib.auth import get_user_model
from django.db import connection
from django.test import TestCase

from operators.models import Operator
from tenant_settings.audit import diff_overrides
from tenant_settings.forms import FEATURE_ORDER

DEV_TENANT_ID = uuid.UUID("00000000-0000-0000-0000-000000000002")
CHANGE_URL = f"/admin/tenant_settings/tenantsetting/{DEV_TENANT_ID}/change/"
ADD_URL = "/admin/tenant_settings/tenantsetting/add/"


# ---------------------------------------------------------------------------
# DB setup helpers
# ---------------------------------------------------------------------------


def _create_test_tables() -> None:
    """
    Create unmanaged tables in the test database.
    Safe to run against an already-migrated stack (IF NOT EXISTS).
    """
    with connection.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        cur.execute("""
            CREATE TABLE IF NOT EXISTS tenants (
                id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
                slug        text        UNIQUE NOT NULL,
                name        text        NOT NULL,
                settings    jsonb       NOT NULL DEFAULT '{}',
                created_at  timestamptz NOT NULL DEFAULT now()
            )
        """)
        cur.execute("""
            CREATE TABLE IF NOT EXISTS tenant_settings (
                id                     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id              uuid        NOT NULL UNIQUE,
                feature_tier_overrides jsonb       NOT NULL DEFAULT '{}',
                created_at             timestamptz NOT NULL DEFAULT now(),
                updated_at             timestamptz NOT NULL DEFAULT now()
            )
        """)
        cur.execute("""
            CREATE TABLE IF NOT EXISTS audit_log (
                id          bigserial   PRIMARY KEY,
                tenant_id   uuid,
                subject_id  text,
                table_name  text        NOT NULL,
                row_pk      text,
                action      text        NOT NULL,
                reason_code text,
                ts          timestamptz NOT NULL DEFAULT now(),
                latency_ms  integer,
                cost_micros integer
            )
        """)


def _seed_tenant(tenant_id: uuid.UUID = DEV_TENANT_ID) -> None:
    with connection.cursor() as cur:
        cur.execute(
            """
            INSERT INTO tenants (id, slug, name)
            VALUES (%s, %s, %s)
            ON CONFLICT (id) DO NOTHING
            """,
            [str(tenant_id), f"test-{tenant_id}", f"Test Tenant {tenant_id}"],
        )


def _upsert_tenant_setting(tenant_id: uuid.UUID, overrides: dict) -> None:
    import json

    with connection.cursor() as cur:
        cur.execute(
            """
            INSERT INTO tenant_settings (tenant_id, feature_tier_overrides)
            VALUES (%s, %s::jsonb)
            ON CONFLICT (tenant_id) DO UPDATE
                SET feature_tier_overrides = EXCLUDED.feature_tier_overrides,
                    updated_at = now()
            """,
            [str(tenant_id), json.dumps(overrides)],
        )


def _audit_row_count(tenant_id: uuid.UUID) -> int:
    with connection.cursor() as cur:
        cur.execute(
            "SELECT COUNT(*) FROM audit_log WHERE tenant_id = %s AND action = %s",
            [str(tenant_id), "tenant_settings.override_change"],
        )
        return cur.fetchone()[0]


def _build_post_data(disabled: list[str] | None = None, tier_overrides: dict | None = None) -> dict:
    """
    Build a valid admin change-form POST payload.

    ``tier_overrides`` maps feature_name → tier string (e.g. "TIER_C").
    All tier_override_* fields not in ``tier_overrides`` default to "" (no override).
    """
    disabled = disabled or []
    tier_overrides = tier_overrides or {}
    data: dict = {}
    for f in disabled:
        data.setdefault("disabled_features", []).append(f)
    for feat in FEATURE_ORDER:
        data[f"tier_override_{feat}"] = tier_overrides.get(feat, "")
    return data


# ---------------------------------------------------------------------------
# Test class
# ---------------------------------------------------------------------------


class TenantSettingAdminTest(TestCase):
    """
    Integration tests for TenantSettingAdmin.

    REQUIRES POSTGRES — all tests touch the DB.
    Tables are created in setUpClass (outside Django's per-test transaction
    savepoint) so they persist for the full test class run.
    """

    databases = {"default"}

    @classmethod
    def setUpClass(cls) -> None:
        super().setUpClass()
        _create_test_tables()

    def setUp(self) -> None:
        User = get_user_model()

        # Admin operator (role=admin, scoped to DEV_TENANT_ID).
        self.admin_user = User.objects.create_user(
            username="ts-admin",
            email="ts-admin@example.test",
            password="x",
            is_staff=True,
            is_superuser=True,  # required to access /admin/
        )
        Operator.objects.create(
            email="ts-admin@example.test",
            tenant_id=DEV_TENANT_ID,
            role=Operator.ROLE_ADMIN,
        )

        # Viewer operator (role=viewer, scoped to DEV_TENANT_ID).
        self.viewer_user = User.objects.create_user(
            username="ts-viewer",
            email="ts-viewer@example.test",
            password="x",
            is_staff=True,
            is_superuser=True,  # /admin/ requires is_staff; RBAC is layered on top
        )
        Operator.objects.create(
            email="ts-viewer@example.test",
            tenant_id=DEV_TENANT_ID,
            role=Operator.ROLE_VIEWER,
        )

        _seed_tenant(DEV_TENANT_ID)
        _upsert_tenant_setting(DEV_TENANT_ID, {})

    # ------------------------------------------------------------------
    # Test 1: change form renders all 7 feature names
    # ------------------------------------------------------------------

    def test_change_form_shows_all_feature_names(self) -> None:
        """
        GET the change form — assert all 7 Tier-A/B feature names appear in
        the disabled-features multiselect and in the tier-override rows.
        """
        self.client.force_login(self.admin_user)
        response = self.client.get(CHANGE_URL)

        self.assertEqual(response.status_code, 200)
        for feat in FEATURE_ORDER:
            self.assertContains(
                response,
                feat,
                msg_prefix=f"Feature '{feat}' not found in change-form HTML",
            )

    # ------------------------------------------------------------------
    # Test 2: save with two disabled features → DB update + 2 audit rows
    # ------------------------------------------------------------------

    def test_save_two_disabled_features_writes_audit_rows(self) -> None:
        """
        POST with two disabled features.  Assert:
        - The tenant_settings row has disabled_features = ["attorney_win_rate", "judge_severity"]
        - Exactly 2 audit_log rows are written with action='tenant_settings.override_change'
        """
        # REQUIRES POSTGRES: hits tenant_settings + audit_log.
        self.client.force_login(self.admin_user)

        disabled_to_set = ["judge_severity", "attorney_win_rate"]
        post_data = _build_post_data(disabled=disabled_to_set)

        audit_before = _audit_row_count(DEV_TENANT_ID)
        response = self.client.post(CHANGE_URL, data=post_data, follow=True)

        # Admin redirects to changelist on success (200 after follow).
        self.assertEqual(response.status_code, 200)

        # Verify DB row updated.
        with connection.cursor() as cur:
            cur.execute(
                "SELECT feature_tier_overrides FROM tenant_settings WHERE tenant_id = %s",
                [str(DEV_TENANT_ID)],
            )
            row = cur.fetchone()
        self.assertIsNotNone(row, "tenant_settings row not found")
        import json
        saved = json.loads(row[0]) if isinstance(row[0], str) else row[0]
        self.assertCountEqual(
            saved.get("disabled_features", []),
            disabled_to_set,
            "disabled_features mismatch in DB",
        )

        # Verify 2 audit rows were written.
        audit_after = _audit_row_count(DEV_TENANT_ID)
        self.assertEqual(
            audit_after - audit_before,
            2,
            f"Expected 2 new audit rows, got {audit_after - audit_before}",
        )

    # ------------------------------------------------------------------
    # Test 3: idempotent re-save writes zero audit rows
    # ------------------------------------------------------------------

    def test_idempotent_save_writes_zero_audit_rows(self) -> None:
        """
        Save the same overrides twice.  The second save must write 0 audit rows.
        """
        # REQUIRES POSTGRES.
        self.client.force_login(self.admin_user)

        disabled_to_set = ["materiality_score"]
        post_data = _build_post_data(disabled=disabled_to_set)

        # First save — expected to write 1 audit row.
        self.client.post(CHANGE_URL, data=post_data, follow=True)
        audit_after_first = _audit_row_count(DEV_TENANT_ID)

        # Second save — identical data, expected to write 0 audit rows.
        response = self.client.post(CHANGE_URL, data=post_data, follow=True)
        self.assertEqual(response.status_code, 200)

        audit_after_second = _audit_row_count(DEV_TENANT_ID)
        self.assertEqual(
            audit_after_second - audit_after_first,
            0,
            "Idempotent re-save must write zero audit rows",
        )

    # ------------------------------------------------------------------
    # Test 4: tier upgrade attempt (TIER_A) is rejected with a form error
    # ------------------------------------------------------------------

    def test_tier_upgrade_attempt_is_rejected(self) -> None:
        """
        POST a tier override of TIER_A.  Assert:
        - The response re-renders the form (no redirect → validation error).
        - The DB row is NOT changed.
        """
        # REQUIRES POSTGRES.
        # First, set a known state in the DB.
        _upsert_tenant_setting(DEV_TENANT_ID, {"disabled_features": [], "tier_overrides": {}})

        self.client.force_login(self.admin_user)
        post_data = _build_post_data(tier_overrides={"judge_severity": "TIER_A"})
        response = self.client.post(CHANGE_URL, data=post_data)

        # Form validation errors: admin re-renders the form (status 200, no redirect).
        self.assertEqual(response.status_code, 200)
        # The response should contain an error message about TIER_A.
        self.assertContains(response, "TIER_A", msg_prefix="Expected TIER_A error in response")

        # DB should be unchanged.
        with connection.cursor() as cur:
            cur.execute(
                "SELECT feature_tier_overrides FROM tenant_settings WHERE tenant_id = %s",
                [str(DEV_TENANT_ID)],
            )
            row = cur.fetchone()
        import json
        saved = json.loads(row[0]) if isinstance(row[0], str) else row[0]
        self.assertEqual(
            saved.get("tier_overrides", {}),
            {},
            "DB must not be updated when a TIER_A upgrade is submitted",
        )

    # ------------------------------------------------------------------
    # Test 5: unknown feature name in disabled_features is rejected
    # ------------------------------------------------------------------

    def test_unknown_disabled_feature_is_rejected(self) -> None:
        """
        POST with a disabled_features value that is not in FEATURE_ORDER.
        This tests the server-side validation (the widget restricts choices,
        but malformed POST data must also be caught).
        """
        # REQUIRES POSTGRES.
        self.client.force_login(self.admin_user)

        # Directly craft a POST bypassing the widget's choice restriction.
        post_data = _build_post_data()
        post_data["disabled_features"] = ["NOT_A_REAL_FEATURE"]

        response = self.client.post(CHANGE_URL, data=post_data)
        # Validation error → form re-rendered (200, no redirect).
        self.assertEqual(response.status_code, 200)
        self.assertContains(
            response,
            "Select a valid choice",
            msg_prefix="Expected Django 'Select a valid choice' error for unknown feature",
        )

    # ------------------------------------------------------------------
    # Test 6: viewer operator cannot save (read-only)
    # ------------------------------------------------------------------

    def test_viewer_cannot_save(self) -> None:
        """
        A viewer operator POSTing to the change form must receive a 403
        (has_change_permission returns False for role=viewer).
        The DB row must not be modified.
        """
        # REQUIRES POSTGRES.
        _upsert_tenant_setting(DEV_TENANT_ID, {"disabled_features": [], "tier_overrides": {}})

        self.client.force_login(self.viewer_user)
        post_data = _build_post_data(disabled=["judge_severity"])
        response = self.client.post(CHANGE_URL, data=post_data)

        self.assertEqual(
            response.status_code,
            403,
            "Viewer POST must return 403 (no change permission)",
        )

        # DB must be unchanged.
        with connection.cursor() as cur:
            cur.execute(
                "SELECT feature_tier_overrides FROM tenant_settings WHERE tenant_id = %s",
                [str(DEV_TENANT_ID)],
            )
            row = cur.fetchone()
        import json
        saved = json.loads(row[0]) if isinstance(row[0], str) else row[0]
        self.assertEqual(
            saved.get("disabled_features", []),
            [],
            "DB must not be updated when a viewer submits the form",
        )


# ---------------------------------------------------------------------------
# Pure-logic unit tests (no DB required)
# ---------------------------------------------------------------------------


class DiffOverridesUnitTest(TestCase):
    """
    Unit tests for audit.diff_overrides().  No DB required.
    """

    def test_empty_dicts_produce_no_diff(self) -> None:
        self.assertEqual(diff_overrides({}, {}), [])

    def test_adding_disabled_feature_is_detected(self) -> None:
        old = {"disabled_features": [], "tier_overrides": {}}
        new = {"disabled_features": ["judge_severity"], "tier_overrides": {}}
        changed = diff_overrides(old, new)
        self.assertIn("judge_severity", changed)
        self.assertEqual(len(changed), 1)

    def test_removing_disabled_feature_is_detected(self) -> None:
        old = {"disabled_features": ["attorney_win_rate"], "tier_overrides": {}}
        new = {"disabled_features": [], "tier_overrides": {}}
        changed = diff_overrides(old, new)
        self.assertIn("attorney_win_rate", changed)

    def test_tier_override_value_change_is_detected(self) -> None:
        old = {"disabled_features": [], "tier_overrides": {"case_type": "TIER_B"}}
        new = {"disabled_features": [], "tier_overrides": {"case_type": "TIER_C"}}
        changed = diff_overrides(old, new)
        self.assertIn("case_type", changed)

    def test_identical_overrides_produce_empty_diff(self) -> None:
        overrides = {
            "disabled_features": ["jurisdiction"],
            "tier_overrides": {"materiality_score": "TIER_C"},
        }
        self.assertEqual(diff_overrides(overrides, overrides), [])
