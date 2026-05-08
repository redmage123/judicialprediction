"""
Audit recorder for ml-inference-svc.

Records outbound ML inference calls to the ``audit_log`` Postgres table using
the same schema as the Rust ``audit-recorder`` crate (S2.11, JP-34).

Design decisions
----------------
- Async, using asyncpg for direct Postgres access (no ORM overhead on the hot path).
- Fire-and-forget: ``record()`` launches a background task and returns immediately
  so the /predict response is never held up by an audit INSERT.
- Graceful degradation: if asyncpg is not installed, DATABASE_URL is not set, or
  the DB connection fails, the recorder is disabled at import time and all
  ``record()`` calls become no-ops.  The inference endpoint keeps working.
- Privacy-preserving: only a SHA-256 hex digest of the request payload is stored,
  never the payload itself (§13 of the spec).

Column mapping (matches Rust audit-recorder)
--------------------------------------------
  actor       → subject_id
  action      → action
  payload_hash→ row_pk        (SHA-256 hex)
  status      → reason_code   ("ok" | "err" | "timeout" | "rate_limit")
  latency_ms  → latency_ms
  cost_micros → cost_micros   (None → NULL; LLM token cost in microdollars)
  table_name  always "outbound_call"
"""
from __future__ import annotations

import asyncio
import hashlib
import logging
import os
import time
from typing import Optional

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Optional asyncpg import — degrade gracefully if not installed
# ---------------------------------------------------------------------------

try:
    import asyncpg  # type: ignore[import]
    _ASYNCPG_AVAILABLE = True
except ImportError:  # pragma: no cover
    asyncpg = None  # type: ignore[assignment]
    _ASYNCPG_AVAILABLE = False
    logger.debug("asyncpg not installed — audit recording disabled")


# ---------------------------------------------------------------------------
# Status constants (mirror Rust AuditStatus::as_str())
# ---------------------------------------------------------------------------

STATUS_OK = "ok"
STATUS_ERR = "err"
STATUS_TIMEOUT = "timeout"
STATUS_RATE_LIMIT = "rate_limit"


# ---------------------------------------------------------------------------
# Payload hashing — deterministic SHA-256 hex (matches Rust hash_payload)
# ---------------------------------------------------------------------------

def hash_payload(data: bytes) -> str:
    """Return the SHA-256 hex digest of *data* (64 hex chars)."""
    return hashlib.sha256(data).hexdigest()


# ---------------------------------------------------------------------------
# Connection pool (module-level singleton, lazy-initialised)
# ---------------------------------------------------------------------------

_pool: Optional[object] = None  # asyncpg.Pool when connected
_pool_lock: Optional[asyncio.Lock] = None
_disabled: bool = False  # set True on connection failure


def _db_url() -> Optional[str]:
    return os.environ.get("AUDIT_DATABASE_URL") or os.environ.get("DATABASE_URL")


async def _get_pool() -> Optional[object]:
    """Return the shared asyncpg pool, connecting on first call.

    Returns None if asyncpg is unavailable, the env var is not set, or the
    connection fails.  Never raises.
    """
    global _pool, _pool_lock, _disabled

    if _disabled or not _ASYNCPG_AVAILABLE:
        return None

    # Lazy-init the lock (must happen inside an event loop).
    if _pool_lock is None:
        _pool_lock = asyncio.Lock()

    async with _pool_lock:
        if _pool is not None:
            return _pool
        if _disabled:
            return None

        url = _db_url()
        if not url:
            logger.debug("AUDIT_DATABASE_URL / DATABASE_URL not set — audit recording disabled")
            _disabled = True
            return None

        try:
            _pool = await asyncpg.create_pool(url, min_size=1, max_size=3)
            logger.info("audit recorder pool connected")
        except Exception as exc:
            logger.warning("audit recorder pool failed to connect (%s) — recording disabled", exc)
            _disabled = True
            return None

    return _pool


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

async def record(
    *,
    tenant_id: str,
    actor: str,
    action: str,
    payload_hash: str,
    latency_ms: int,
    status: str = STATUS_OK,
    cost_micros: Optional[int] = None,
) -> None:
    """Insert one audit event into ``audit_log``, scoped to *tenant_id*.

    Runs ``SET LOCAL app.current_tenant_id`` inside a short transaction so
    the RLS insert-policy is satisfied.  Non-fatal: any exception is logged
    at WARNING level and swallowed.

    Callers should use ``asyncio.create_task(record(...))`` for fire-and-forget
    behaviour (see ``record_fire_and_forget``).
    """
    pool = await _get_pool()
    if pool is None:
        return

    try:
        async with pool.acquire() as conn:
            async with conn.transaction():
                await conn.execute(
                    f"SET LOCAL app.current_tenant_id = '{tenant_id}'"
                )
                await conn.execute(
                    """
                    INSERT INTO audit_log
                        (tenant_id, subject_id, table_name, row_pk, action,
                         reason_code, latency_ms, cost_micros)
                    VALUES
                        ($1, $2, $3, $4, $5, $6, $7, $8)
                    """,
                    tenant_id,
                    actor,
                    "outbound_call",
                    payload_hash,
                    action,
                    status,
                    latency_ms,
                    cost_micros,
                )
    except Exception as exc:
        logger.warning("audit record failed (non-fatal): %s", exc)


def record_fire_and_forget(
    *,
    tenant_id: str,
    actor: str,
    action: str,
    payload_hash: str,
    latency_ms: int,
    status: str = STATUS_OK,
    cost_micros: Optional[int] = None,
) -> None:
    """Schedule an audit INSERT as a background asyncio task.

    Returns immediately — the /predict response is never blocked.
    The background task swallows all errors (audit failures must not
    affect the inference response).
    """
    try:
        loop = asyncio.get_event_loop()
        loop.create_task(
            record(
                tenant_id=tenant_id,
                actor=actor,
                action=action,
                payload_hash=payload_hash,
                latency_ms=latency_ms,
                status=status,
                cost_micros=cost_micros,
            )
        )
    except RuntimeError:
        # No running event loop (e.g. during unit tests without asyncio) — skip.
        pass
