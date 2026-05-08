// FUNCTIONAL-CORE note: AuditEvent + AuditStatus are pure value types with no I/O.
// AuditRecorder is the imperative shell (owns a Postgres pool, does I/O).

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain types — pure, no I/O
// ---------------------------------------------------------------------------

/// Outcome status of an auditable outbound call.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    Ok,
    Err,
    Timeout,
    RateLimit,
}

impl AuditStatus {
    /// Stable string representation stored in `audit_log.reason_code`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Ok => "ok",
            AuditStatus::Err => "err",
            AuditStatus::Timeout => "timeout",
            AuditStatus::RateLimit => "rate_limit",
        }
    }
}

/// A single auditable outbound call event.
///
/// Maps onto `audit_log` columns:
/// - `actor`        → `subject_id`
/// - `action`       → `action`
/// - `payload_hash` → `row_pk`
/// - `status`       → `reason_code`
/// - `latency_ms`   → `latency_ms`  (new column, migration 20260507120004)
/// - `cost_micros`  → `cost_micros` (new column, migration 20260507120004)
/// Service category is always logged as `table_name = 'outbound_call'`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuditEvent {
    /// Service principal making the call (e.g. "api-gateway", "ml-inference-svc").
    pub actor: String,
    /// Fully-qualified action / RPC name (e.g. "feature_store.GetFeature").
    pub action: String,
    /// SHA-256 hex digest of the serialised request payload.
    pub payload_hash: String,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u32,
    /// Outcome of the call.
    pub status: AuditStatus,
    /// LLM token cost or third-party API credits in microdollars. `None` when
    /// not applicable (e.g. internal gRPC calls with no per-call pricing).
    pub cost_micros: Option<u32>,
}

// ---------------------------------------------------------------------------
// Hashing helper — pure function, deterministic
// ---------------------------------------------------------------------------

/// Compute the SHA-256 hex digest of an arbitrary byte slice.
///
/// Deterministic: identical inputs always produce identical digests.
/// Used to record a tamper-evident fingerprint of the request payload without
/// storing the payload itself (privacy-preserving per §13 of the spec).
pub fn hash_payload(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Recorder — imperative shell
// ---------------------------------------------------------------------------

/// Records auditable outbound-call events into the `audit_log` Postgres table.
///
/// Every call to [`record`] opens a short transaction, sets
/// `app.current_tenant_id` via `SET LOCAL` so the RLS insert-policy
/// enforces tenant isolation, inserts the row, and commits.
///
/// `AuditRecorder` is `Clone`; cloning is cheap because `PgPool` is an
/// `Arc`-backed connection pool.
#[derive(Clone)]
pub struct AuditRecorder {
    pool: PgPool,
}

impl AuditRecorder {
    /// Construct from an existing pool (e.g. shared with the feature-store repo).
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Connect to Postgres at `database_url` and return a ready recorder.
    ///
    /// The caller does not need to import `sqlx` — the connection is managed
    /// entirely inside this crate.
    pub async fn new_from_url(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url)
            .await
            .context("audit-recorder: connect to Postgres")?;
        Ok(Self { pool })
    }

    /// Insert one audit event, scoped to `tenant_id`.
    ///
    /// Opens a transaction, sets `app.current_tenant_id = tenant_id`, inserts
    /// the audit row, and commits.  Returns `Err` if Postgres rejects the
    /// insert (e.g. RLS policy violation, schema mismatch).
    pub async fn record(&self, tenant_id: Uuid, event: AuditEvent) -> Result<()> {
        let mut tx = self.pool.begin().await.context("audit: begin tx")?;

        // SET LOCAL is scoped to this transaction. `Uuid::to_string()` always
        // produces a well-formed UUID literal — no injection risk.
        let set_sql = format!("SET LOCAL app.current_tenant_id = '{tenant_id}'");
        sqlx::query(&set_sql)
            .execute(&mut *tx)
            .await
            .context("audit: SET LOCAL tenant")?;

        sqlx::query(
            r#"
            INSERT INTO audit_log
                (tenant_id, subject_id, table_name, row_pk, action,
                 reason_code, latency_ms, cost_micros)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(tenant_id)
        .bind(&event.actor)
        .bind("outbound_call")        // table_name: category for outbound audit events
        .bind(&event.payload_hash)    // row_pk:     SHA-256 of the request payload
        .bind(&event.action)
        .bind(event.status.as_str())  // reason_code: AuditStatus enum string
        .bind(event.latency_ms as i32)
        .bind(event.cost_micros.map(|c| c as i32))
        .execute(&mut *tx)
        .await
        .context("audit: INSERT into audit_log")?;

        tx.commit().await.context("audit: commit")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — pure logic only, no I/O
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `hash_payload` is deterministic: same bytes → same digest every time.
    #[test]
    fn hash_is_deterministic() {
        let a = hash_payload(b"hello world");
        let b = hash_payload(b"hello world");
        assert_eq!(a, b, "hash must be stable across calls");
        // SHA-256 hex is always 64 hex characters.
        assert_eq!(a.len(), 64, "SHA-256 hex must be exactly 64 chars");
    }

    /// Different inputs produce different digests (collision-resistance smoke check).
    #[test]
    fn hash_differs_for_different_inputs() {
        let a = hash_payload(b"request-payload-A");
        let b = hash_payload(b"request-payload-B");
        assert_ne!(a, b, "distinct payloads must not hash to the same digest");
    }

    /// `AuditEvent` round-trips through JSON without loss.
    #[test]
    fn audit_event_json_roundtrip() {
        let event = AuditEvent {
            actor: "api-gateway".to_string(),
            action: "feature_store.GetFeature".to_string(),
            payload_hash: hash_payload(b"test-payload"),
            latency_ms: 42,
            status: AuditStatus::Ok,
            cost_micros: Some(750),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: AuditEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.actor, event.actor);
        assert_eq!(decoded.action, event.action);
        assert_eq!(decoded.payload_hash, event.payload_hash);
        assert_eq!(decoded.latency_ms, event.latency_ms);
        assert_eq!(decoded.status, event.status);
        assert_eq!(decoded.cost_micros, event.cost_micros);
    }

    /// `AuditStatus` covers all four variants; `as_str` is exhaustive and stable.
    #[test]
    fn audit_status_all_variants_and_strings() {
        let cases = [
            (AuditStatus::Ok, "ok"),
            (AuditStatus::Err, "err"),
            (AuditStatus::Timeout, "timeout"),
            (AuditStatus::RateLimit, "rate_limit"),
        ];
        for (variant, expected_str) in &cases {
            assert_eq!(
                variant.as_str(),
                *expected_str,
                "AuditStatus::{:?} must produce {:?}",
                variant,
                expected_str
            );
        }
    }
}
