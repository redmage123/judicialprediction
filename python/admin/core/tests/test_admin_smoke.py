"""
Admin smoke test — S2.15 (Plane JP-39).

Verifies that the Django admin console boots, the Tenant list view returns
HTTP 200, and at least one tenant row is rendered.

Requirements:
    - A running Postgres instance reachable via ADMIN_DATABASE_URL
      (defaults to the docker-compose dev stack on 127.0.0.1:5454).
    - ``uv run pytest`` from the ``python/admin/`` directory.

Run:
    cd python/admin
    uv run pytest core/tests/test_admin_smoke.py -v

The test creates unmanaged application tables (tenants, cases, users) in the
test database because Django's migration runner skips ``managed=False`` models.
Tables are created with CREATE TABLE IF NOT EXISTS so the test is idempotent
against the dev Postgres stack.
"""

from django.contrib.auth import get_user_model
from django.db import connection
from django.test import TestCase


class TenantAdminSmokeTest(TestCase):
    """
    Smoke test for ``GET /admin/core/tenant/``.

    Uses ``django.test.TestCase`` (wraps each test in a savepoint transaction
    that is rolled back after the test, keeping the test database clean).

    ``setUpClass`` creates the unmanaged application tables once per class
    because Django's test runner does not run migrations for ``managed=False``
    models.  The ``CREATE TABLE IF NOT EXISTS`` statements are idempotent and
    safe to run against an already-migrated dev Postgres.
    """

    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        _create_unmanaged_tables()

    def setUp(self):
        User = get_user_model()
        self.admin_user = User.objects.create_superuser(
            username="devadmin",
            email="dev@judicialpredict.ai",
            password="smoke-test-pass-not-for-prod",
        )
        _seed_dev_tenant()

    def test_tenant_list_returns_200_and_renders_seed_row(self):
        """
        ``GET /admin/core/tenant/`` must return 200 and render the seed tenant.

        Asserts:
            - HTTP status is 200.
            - The response body contains "Dev Tenant" (the seed tenant name).
        """
        self.client.force_login(self.admin_user)
        response = self.client.get("/admin/core/tenant/")
        self.assertEqual(response.status_code, 200)
        self.assertContains(response, "Dev Tenant")


# ---------------------------------------------------------------------------
# Test helpers
# ---------------------------------------------------------------------------


def _create_unmanaged_tables() -> None:
    """
    Create application tables that Django's migration runner skips.

    Uses ``CREATE TABLE IF NOT EXISTS`` so this is safe to run against
    the live dev Postgres stack (tables already exist there).

    The ``users`` table is a Sprint-3 placeholder — it does not exist in
    the baseline Rust migration.  The stub schema here unblocks the smoke
    test; the real schema will be defined in a Rust migration.
    """
    with connection.cursor() as cursor:
        # pgcrypto provides gen_random_uuid() used as the DEFAULT for id columns.
        cursor.execute("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS tenants (
                id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
                slug        text        UNIQUE NOT NULL,
                name        text        NOT NULL,
                settings    jsonb       NOT NULL DEFAULT '{}',
                created_at  timestamptz NOT NULL DEFAULT now()
            )
        """)
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS cases (
                id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id   uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                title       text        NOT NULL,
                jurisdiction text       NOT NULL,
                court       text,
                judge_name  text,
                parties     jsonb       NOT NULL DEFAULT '{}',
                claims      jsonb       NOT NULL DEFAULT '[]',
                created_at  timestamptz NOT NULL DEFAULT now(),
                updated_at  timestamptz NOT NULL DEFAULT now()
            )
        """)
        # Sprint-3 Rust migration will define the production schema.
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS users (
                id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id   uuid        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                email       text        UNIQUE NOT NULL,
                role        text        NOT NULL DEFAULT 'member',
                created_at  timestamptz NOT NULL DEFAULT now()
            )
        """)


def _seed_dev_tenant() -> None:
    """Insert the canonical dev tenant if it does not already exist."""
    with connection.cursor() as cursor:
        cursor.execute(
            """
            INSERT INTO tenants (id, slug, name)
            VALUES ('00000000-0000-0000-0000-000000000001', 'dev', 'Dev Tenant')
            ON CONFLICT (id) DO NOTHING
            """,
        )
