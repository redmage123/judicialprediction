# =============================================================================
# ml-inference-svc — multi-stage build
# Stage 1: builder — installs deps with uv
# Stage 2: runner — minimal runtime image
# =============================================================================

# ---- builder ----------------------------------------------------------------
FROM python:3.12-slim AS builder

# Install uv
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

WORKDIR /build

# Copy dependency manifests first (layer-cache friendly).
COPY pyproject.toml .
COPY src/ src/

# Install deps into a local venv (no editable install in the image).
RUN uv sync --no-dev --no-editable

# ---- runner -----------------------------------------------------------------
FROM python:3.12-slim AS runner

# libgomp1 = OpenMP runtime; required by LightGBM (loaded via ctypes at import).
RUN apt-get update \
    && apt-get install -y --no-install-recommends libgomp1 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1000 appgroup \
    && useradd --uid 1000 --gid appgroup --no-create-home --shell /usr/sbin/nologin appuser

WORKDIR /app

# Copy the installed venv from builder.
COPY --from=builder /build/.venv /app/.venv

# Copy application source.
COPY --from=builder /build/src /app/src

# Put venv on PATH.
ENV PATH="/app/.venv/bin:$PATH"
ENV PYTHONPATH="/app/src"

# Run as UID 1000 so the bind-mounted mlruns/ stays writable by both the
# container (training, eval) AND the host operator running scripts/train*.py
# directly. UID 1000 matches the default host developer UID we provision on
# all dev boxes; K8s pods in prod use the same UID in their PodSecurityContext.
USER appuser

# Liveness / readiness ports.
EXPOSE 8001

CMD ["python", "-m", "uvicorn", "ml_inference_svc.main:app", "--host", "0.0.0.0", "--port", "8001"]
