# JudicialPredict Admin Console — multi-stage Docker build
#
# Build:  docker build -t jp-admin .
# Run:    docker run -p 8000:8000 \
#           -e ADMIN_DATABASE_URL=postgres://... \
#           -e DJANGO_SECRET_KEY=... \
#           jp-admin

# ---------------------------------------------------------------------------
# Stage 1: install Python dependencies with uv
# ---------------------------------------------------------------------------
FROM python:3.12-slim AS builder

WORKDIR /app

# Install uv.
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

# Force uv to use the system Python (3.12 from the base image) rather
# than downloading its own managed interpreter.  The runtime stage uses
# the same base image, so `/app/.venv/bin/python` (a symlink) resolves
# to the same path in both stages.  Without this, recent uv versions
# install a managed Python and the runtime symlink becomes a dangling
# pointer — `python -V` fails and Django can't be imported.
ENV UV_PYTHON_PREFERENCE=only-system

# Copy dependency manifests first (cache layer).
# uv.lock is required — without it `uv sync` resolves to an empty install
# and the runtime stage fails with `ImportError: Couldn't import Django`.
COPY pyproject.toml uv.lock ./

# Install runtime deps only (no dev extras), into /app/.venv.
RUN uv sync --no-dev --no-install-project

# ---------------------------------------------------------------------------
# Stage 2: runtime image
# ---------------------------------------------------------------------------
FROM python:3.12-slim AS runtime

WORKDIR /app

# Copy the pre-built venv from the builder stage.
COPY --from=builder /app/.venv /app/.venv

# Copy application source.
COPY . .

# Activate the uv-managed venv.
ENV PATH="/app/.venv/bin:$PATH"
ENV DJANGO_SETTINGS_MODULE="judicialpredict_admin.settings"
ENV PYTHONDONTWRITEBYTECODE=1
ENV PYTHONUNBUFFERED=1

# NOTE — `collectstatic` is deliberately NOT run at build time.
# `manage.py runserver` serves static assets directly when DEBUG=True
# (the dev path), and a separate prod Dockerfile / Gunicorn entry point
# is the right place to bake in collected static for prod images.
# Running it here breaks the dev build whenever `uv sync` lands the venv
# at a path the runtime stage can't read (lockfile drift, etc.).

EXPOSE 8000

# Run with Gunicorn in production; use runserver for development override.
CMD ["python", "manage.py", "runserver", "0.0.0.0:8000"]
