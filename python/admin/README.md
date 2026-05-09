# JudicialPredict Admin Console

Django 5 operator console for JudicialPredict. Provides CRUD views for
Tenant, Case, and User records via Django Admin. All models use
`managed = False` — the schema is owned by the Rust migration system in
`rust/feature-store/migrations/`.

## Dev Setup

```sh
# From python/admin/
uv sync                                    # install deps into .venv
uv run python manage.py migrate            # create Django-internal tables (auth, sessions)
uv run python manage.py createsuperuser    # create your operator login
uv run python manage.py runserver          # http://127.0.0.1:8000/admin/
```

Requires a running Postgres instance. The default DSN connects to the
docker-compose dev stack on `127.0.0.1:5454`:

```sh
docker compose -f ../../docker-compose.dev.yml up -d postgres
```

Override with `ADMIN_DATABASE_URL`:

```sh
ADMIN_DATABASE_URL=postgres://user:pass@host:5432/db uv run python manage.py runserver
```

## Docker (via docker-compose)

```sh
# Start Postgres + admin console:
docker compose -f ../../docker-compose.dev.yml up admin

# Create a superuser inside the container:
docker compose -f ../../docker-compose.dev.yml exec admin \
    python manage.py createsuperuser
```

Admin is available at `http://localhost:8000/admin/`.

## RLS-Aware Connection

Django connects to Postgres as the superuser (dev default) which bypasses
Row-Level Security. The `RLSMiddleware` (`core/middleware.py`) sets
`app.current_tenant_id` via `SET CONFIG` on every authenticated request —
this is the seam that Sprint-3 will use to enforce per-operator row visibility
once the connection role is changed.

### How it works

1. `RLSMiddleware.process_view` runs after `AuthenticationMiddleware`.
2. It calls `_resolve_tenant(request)` to determine the operator's tenant scope.
3. It issues `SELECT set_config('app.current_tenant_id', <uuid>, false)`.
4. Postgres RLS policies evaluate `current_setting('app.current_tenant_id')` on
   every query from that connection.

**Current scaffold**: all authenticated operators see the dev tenant
(`00000000-0000-0000-0000-000000000001`). A banner in the UI flags this.

### Swap to `jp_admin` role (Sprint-3)

1. Add a Rust migration creating a `jp_admin` Postgres role with `BYPASSRLS`.
2. Set `ADMIN_DATABASE_URL` to a `jp_admin` DSN.
3. Replace the hard-coded `_DEV_TENANT_ID` in `core/middleware.py` with an
   RBAC lookup against a `operator_tenants` join table.
4. Enable `ATOMIC_REQUESTS = True` in settings and switch `set_config` to
   `is_local=true` (transaction-scoped) for safe per-request isolation.

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `ADMIN_DATABASE_URL` | no | dev superuser DSN | Postgres connection string |
| `DJANGO_SECRET_KEY` | prod only | insecure dev default | Django secret key |
| `DEBUG` | no | `true` | Django debug mode |
| `ALLOWED_HOSTS` | no | `localhost,127.0.0.1` | Allowed hostnames |
| `STATIC_ROOT` | no | `<BASE_DIR>/staticfiles` | Path for collected static files |

## Tests

```sh
# From python/admin/ — runs against the dev Postgres:
uv run pytest
```

The smoke test (`core/tests/test_admin_smoke.py`) creates the unmanaged tables
with `CREATE TABLE IF NOT EXISTS` (safe against an already-migrated stack),
seeds a dev tenant, and asserts `GET /admin/core/tenant/` returns 200 and
renders at least one tenant row.

## Sprint-3 Follow-ups

- **Operator RBAC**: `jp_admin` Postgres role + `operator_tenants` table; replace
  hard-coded dev tenant in `RLSMiddleware._resolve_tenant`.
- **Audit-log viewer**: read-only `ModelAdmin` for `audit_log` with date-range
  filtering and CSV export.
- **SSO / SAML**: replace Django local auth with SAML2 (django-saml2-auth) or
  OIDC; tie the `sso_sub` claim to the operator record.
- **`jp_admin` Rust migration**: `CREATE ROLE jp_admin BYPASSRLS` + `GRANT` on
  all application tables; update `ADMIN_DATABASE_URL` in docker-compose and K8s.
- **ATOMIC_REQUESTS**: enable + switch `set_config` to `is_local=true`.
- **Read-replica routing**: add `replica` DB alias in `RLSRouter.db_for_read`.
