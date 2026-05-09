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

# Copy dependency manifests first (cache layer).
COPY pyproject.toml .

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

# Collect static assets at build time (uses STATIC_ROOT from settings).
RUN python manage.py collectstatic --noinput

EXPOSE 8000

# Run with Gunicorn in production; use runserver for development override.
CMD ["python", "manage.py", "runserver", "0.0.0.0:8000"]
