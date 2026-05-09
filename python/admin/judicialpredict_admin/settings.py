"""
JudicialPredict Admin Console — Django settings.

Environment variables (see README.md for full list):
    ADMIN_DATABASE_URL        jp_app DSN for tenant-scoped operators
                              (dev default: Postgres superuser so migrations work)
    ADMIN_SUPER_DATABASE_URL  jp_admin DSN for super-operators (BYPASSRLS)
    DJANGO_SECRET_KEY         Secret key (required in production; has insecure dev default)
    DEBUG                     "true" / "false" (default: true)
    ALLOWED_HOSTS             Comma-separated hostnames (default: localhost,127.0.0.1)
    STATIC_ROOT               Collected static files directory (default: /static)

Database connection notes (ADR-003)
-------------------------------------
Two DATABASES aliases are configured:

    default       Used for tenant-scoped operator requests AND for Django-managed
                  table migrations (operators_operator, auth_*, django_* tables).
                  The dev default uses the Postgres superuser so ``manage.py migrate``
                  can CREATE TABLE operators_operator.  In production, run migrations
                  as superuser, then set ADMIN_DATABASE_URL to the jp_app DSN.

    admin_super   jp_admin (BYPASSRLS).  Used only when RLSMiddleware determines
                  the request user has role='super'.
                  TEST: {MIRROR: 'default'} tells Django's test runner to reuse the
                  same test database — no duplicate DB creation in CI.
"""

from pathlib import Path

import environ

BASE_DIR = Path(__file__).resolve().parent.parent

env = environ.Env()
# Read a local .env if present; never required (twelve-factor: env takes priority).
environ.Env.read_env(BASE_DIR / ".env", overwrite=False)

# ---------------------------------------------------------------------------
# Core
# ---------------------------------------------------------------------------

SECRET_KEY = env(
    "DJANGO_SECRET_KEY",
    default="dev-only-insecure-key-CHANGE-BEFORE-PRODUCTION-0000000000000000000",
)

DEBUG = env.bool("DEBUG", default=True)

ALLOWED_HOSTS = env.list("ALLOWED_HOSTS", default=["localhost", "127.0.0.1", "0.0.0.0"])

# ---------------------------------------------------------------------------
# Application definition
# ---------------------------------------------------------------------------

INSTALLED_APPS = [
    "django.contrib.admin",
    "django.contrib.auth",
    "django.contrib.contenttypes",
    "django.contrib.sessions",
    "django.contrib.messages",
    "django.contrib.staticfiles",
    "core",
    "operators",
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
    "django.contrib.auth.middleware.AuthenticationMiddleware",
    # RLS-aware middleware: resolves Operator record, sets DB alias + app.current_tenant_id.
    # Must run after AuthenticationMiddleware so request.user is populated.
    "core.middleware.RLSMiddleware",
    "django.contrib.messages.middleware.MessageMiddleware",
    "django.middleware.clickjacking.XFrameOptionsMiddleware",
]

ROOT_URLCONF = "judicialpredict_admin.urls"

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": [],
        "APP_DIRS": True,
        "OPTIONS": {
            "context_processors": [
                "django.template.context_processors.debug",
                "django.template.context_processors.request",
                "django.contrib.auth.context_processors.auth",
                "django.contrib.messages.context_processors.messages",
            ],
        },
    },
]

WSGI_APPLICATION = "judicialpredict_admin.wsgi.application"
ASGI_APPLICATION = "judicialpredict_admin.asgi.application"

# ---------------------------------------------------------------------------
# Database
#
# Two aliases (ADR-003):
#
#   default       jp_app — subject to RLS.  Also used for all Django-managed
#                 migrations.  Dev default is the Postgres superuser so
#                 ``manage.py migrate`` can create operators_operator.
#                 Production: set ADMIN_DATABASE_URL to the jp_app DSN after
#                 running migrations as the migration user.
#
#   admin_super   jp_admin — BYPASSRLS.  Used only for role='super' operators.
#                 TEST: {MIRROR: 'default'} reuses the default test database
#                 in Django's test runner (no duplicate DB creation).
#
# Do NOT use the superuser DSN in production for either alias.
# ---------------------------------------------------------------------------

_admin_super_db = env.db(
    "ADMIN_SUPER_DATABASE_URL",
    default=(
        "postgres://jp_admin:judicialpredict_admin_pwd"
        "@127.0.0.1:5454/judicialpredict_dev"
    ),
)
# Reuse the same test DB as 'default' — avoids a second createdb in CI.
_admin_super_db["TEST"] = {"MIRROR": "default"}

DATABASES = {
    # Prefer ADMIN_DATABASE_URL; fall back to superuser dev DSN.
    "default": env.db(
        "ADMIN_DATABASE_URL",
        default=(
            "postgres://judicialpredict:judicialpredict_dev_pwd"
            "@127.0.0.1:5454/judicialpredict_dev"
        ),
    ),
    # jp_admin (BYPASSRLS) — for role='super' operators only.
    "admin_super": _admin_super_db,
}

DATABASE_ROUTERS = ["core.db_router.RLSRouter"]

# ATOMIC_REQUESTS wraps each request in a transaction, enabling
# set_config(is_local=True) in RLSMiddleware for transaction-scoped RLS vars.
ATOMIC_REQUESTS = True

# ---------------------------------------------------------------------------
# Auth
# ---------------------------------------------------------------------------

AUTH_PASSWORD_VALIDATORS = [
    {"NAME": "django.contrib.auth.password_validation.UserAttributeSimilarityValidator"},
    {"NAME": "django.contrib.auth.password_validation.MinimumLengthValidator"},
    {"NAME": "django.contrib.auth.password_validation.CommonPasswordValidator"},
    {"NAME": "django.contrib.auth.password_validation.NumericPasswordValidator"},
]

# ---------------------------------------------------------------------------
# Internationalisation
# ---------------------------------------------------------------------------

LANGUAGE_CODE = "en-us"
TIME_ZONE = "UTC"
USE_I18N = True
USE_TZ = True

# ---------------------------------------------------------------------------
# Static files
# ---------------------------------------------------------------------------

STATIC_URL = "/static/"
STATIC_ROOT = env("STATIC_ROOT", default=str(BASE_DIR / "staticfiles"))

# ---------------------------------------------------------------------------
# Miscellaneous
# ---------------------------------------------------------------------------

DEFAULT_AUTO_FIELD = "django.db.models.BigAutoField"
