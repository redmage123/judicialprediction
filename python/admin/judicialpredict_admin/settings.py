"""
JudicialPredict Admin Console — Django settings.

Environment variables (see README.md for full list):
    ADMIN_DATABASE_URL   Postgres DSN for the admin console (default: dev superuser)
    DJANGO_SECRET_KEY    Secret key (required in production; has insecure dev default)
    DEBUG                "true" / "false" (default: true)
    ALLOWED_HOSTS        Comma-separated hostnames (default: localhost,127.0.0.1)
    STATIC_ROOT          Collected static files directory (default: /static)
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
]

MIDDLEWARE = [
    "django.middleware.security.SecurityMiddleware",
    "django.contrib.sessions.middleware.SessionMiddleware",
    "django.middleware.common.CommonMiddleware",
    "django.middleware.csrf.CsrfViewMiddleware",
    "django.contrib.auth.middleware.AuthenticationMiddleware",
    # RLS-aware middleware: sets app.current_tenant_id on the Postgres connection.
    # Sprint-3: replace hard-coded dev tenant with per-operator RBAC resolution.
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
# The admin console connects as the Postgres SUPERUSER by default (dev only).
# The superuser bypasses RLS so the operator can manage all tenants.
#
# Sprint-3 TODO:
#   1. Create a jp_admin Postgres role with BYPASSRLS (Rust migration).
#   2. Set ADMIN_DATABASE_URL to a jp_admin DSN.
#   3. The RLSMiddleware skeleton below will then scope queries per-operator.
#
# Do NOT use the superuser DSN in production.
# ---------------------------------------------------------------------------

DATABASES = {
    "default": env.db(
        # Prefer ADMIN_DATABASE_URL; fall back to superuser dev DSN.
        "ADMIN_DATABASE_URL",
        default=(
            "postgres://judicialpredict:judicialpredict_dev_pwd"
            "@127.0.0.1:5454/judicialpredict_dev"
        ),
    )
}

DATABASE_ROUTERS = ["core.db_router.RLSRouter"]

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
