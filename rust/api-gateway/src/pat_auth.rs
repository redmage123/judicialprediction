//! S6.15 — Personal Access Token (PAT) auth.
//!
//! `Authorization: Bearer pat_<32 hex>` resolves to the same `(operator_id,
//! tenant_id)` the issuing operator carries.  Tokens are stored as SHA-256
//! hex hashes; plaintext is shown exactly once at mint time.
//!
//! This module is a parallel auth backend to the JWT path.  The
//! `jwt_middleware` in `rate_limit.rs` dispatches on the `pat_` prefix:
//!   - `pat_*`     → resolve_pat → synthesize Claims
//!   - otherwise   → decode_jwt
//! Either path lands the same `Claims` + `TenantId` in request extensions,
//! so every downstream resolver / REST handler is auth-backend-agnostic.

use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row as _};

use crate::auth::Claims;

/// Plaintext PAT prefix.  Plaintext format = `pat_` + 32 hex chars; Django
/// mints these via the `mint_pat` management command (S6.15) and the
/// gateway only ever sees them in `Authorization: Bearer pat_*` headers.
pub const PAT_PREFIX: &str = "pat_";

/// SHA-256 hex of the plaintext PAT.  Stored in `personal_access_tokens.token_hash`.
pub fn hash_pat_token(plaintext: &str) -> String {
    let mut h = Sha256::new();
    h.update(plaintext.as_bytes());
    hex::encode(h.finalize())
}

/// Resolve a PAT plaintext to its operator's `Claims`.
///
/// Lookup is via the SHA-256 hash; an unknown / revoked / expired token
/// returns `None` (which the middleware maps to 401).  On a successful
/// lookup `last_used_at` is updated as a fire-and-forget side effect.
pub async fn resolve_pat(pool: &PgPool, plaintext: &str) -> Option<Claims> {
    if !plaintext.starts_with(PAT_PREFIX) {
        return None;
    }
    let hash = hash_pat_token(plaintext);

    let row = sqlx::query(
        r#"
        SELECT pat.id::text   AS pat_id,
               pat.operator_id::text AS operator_id,
               op.tenant_id::text    AS tenant_id
        FROM personal_access_tokens pat
        JOIN operators_operator op ON op.id = pat.operator_id
        WHERE pat.token_hash = $1
          AND pat.revoked_at IS NULL
          AND (pat.expires_at IS NULL OR pat.expires_at > now())
        "#,
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await
    .ok()??;

    let operator_id: String = row.try_get("operator_id").ok()?;
    let tenant_id_raw: Option<String> = row.try_get("tenant_id").ok();
    // operators with role='super' carry tenant_id NULL — those should
    // NEVER mint usable PATs in v1 (no tenant scope to bind the token
    // to).  Treat as auth failure rather than fall back to a magic id.
    let tenant_id = tenant_id_raw?;
    let pat_id: String = row.try_get("pat_id").ok()?;

    // Side-effect: update last_used_at.  Failure is non-fatal.
    let pool_clone = pool.clone();
    let pat_id_for_update = pat_id.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE personal_access_tokens SET last_used_at = now() WHERE id = $1::uuid",
        )
        .bind(&pat_id_for_update)
        .execute(&pool_clone)
        .await;
    });

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as usize;
    Some(Claims {
        sub: operator_id,
        tenant_id,
        scopes: Vec::new(),
        // PATs are stateless from the gateway's POV — the DB row carries
        // the real lifecycle (`revoked_at`, `expires_at`).  We just need
        // `exp` to satisfy code paths that read it; one hour is generous.
        exp: now + 3600,
        iat: now,
        iss: Some(format!("pat:{pat_id}")),
        aud: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_64_hex_chars() {
        let h = hash_pat_token("pat_0123456789abcdef0123456789abcdef");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase())));
    }

    #[test]
    fn hash_is_deterministic() {
        let a = hash_pat_token("pat_deadbeef");
        let b = hash_pat_token("pat_deadbeef");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_changes_when_plaintext_changes() {
        let a = hash_pat_token("pat_aaaa");
        let b = hash_pat_token("pat_aaab");
        assert_ne!(a, b);
    }
}
