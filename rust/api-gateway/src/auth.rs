// JudicialPredict API Gateway — JWT authentication.
//
// Every customer-facing request must carry a Bearer JWT in the Authorization
// header. Tenant identity (tenant_id), subject, and scopes are extracted from
// the validated claims.
//
// HS256 is used for local dev and CI.  ES256 (JWKS endpoint + External Secrets
// Operator) is planned for Sprint 3+; the `decode_jwt` signature is already
// agnostic to the key material type so the migration will be additive.

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims carried on every authenticated request.
///
/// Fields follow the standard JWT vocabulary where applicable:
/// - `sub`       — subject identifier (user or service account id)
/// - `tenant_id` — the tenant UUID this token is scoped to (custom claim)
/// - `scopes`    — permission scopes, e.g. `["features:read", "features:write"]`
/// - `exp`/`iat` — standard expiry / issued-at (UNIX epoch seconds)
/// - `iss`/`aud` — optional issuer / audience (validated in prod; relaxed in dev)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — identifies the end user or service account.
    pub sub: String,
    /// Tenant UUID this token is scoped to. Must be a valid UUID v4 string.
    pub tenant_id: String,
    /// Granted permission scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Expiry time (UNIX epoch seconds). Validated on every request.
    pub exp: usize,
    /// Issued-at time (UNIX epoch seconds).
    pub iat: usize,
    /// Token issuer. Optional in dev; required in prod.
    pub iss: Option<String>,
    /// Intended audience. Stored as a flexible JSON value so single-string and
    /// array-of-strings both deserialise correctly.
    pub aud: Option<serde_json::Value>,
}

/// Decode and verify a JWT using HS256.
///
/// Returns the extracted [`Claims`] on success.
/// Returns an error if the token is malformed, expired, or has an invalid
/// signature.
///
/// Production upgrade path (Sprint 3+): accept an ES256 `DecodingKey` built
/// from a JWKS endpoint; the call-site signature stays the same — pass the
/// appropriate `secret` bytes.
pub fn decode_jwt(token: &str, secret: &[u8]) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    // Audience validation is deferred to Sprint 3+ when we have a JWKS endpoint
    // and a stable `aud` claim in all tokens.
    validation.validate_aud = false;

    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    const SECRET: &[u8] = b"judicialpredict-test-jwt-secret!";

    fn valid_claims(tenant_id: &str) -> Claims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        Claims {
            sub: "test-user".to_string(),
            tenant_id: tenant_id.to_string(),
            scopes: vec!["features:read".to_string()],
            exp: now + 3600,
            iat: now,
            iss: None,
            aud: None,
        }
    }

    #[test]
    fn valid_token_decodes_correctly() {
        let claims = valid_claims("00000000-0000-0000-0000-000000000001");
        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET)).unwrap();
        let decoded = decode_jwt(&token, SECRET).unwrap();
        assert_eq!(decoded.tenant_id, claims.tenant_id);
        assert_eq!(decoded.sub, "test-user");
        assert_eq!(decoded.scopes, vec!["features:read"]);
    }

    #[test]
    fn expired_token_is_rejected() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        let mut claims = valid_claims("00000000-0000-0000-0000-000000000001");
        claims.exp = now - 3600; // 1 hour in the past
        claims.iat = now - 7200;
        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET)).unwrap();
        let result = decode_jwt(&token, SECRET);
        assert!(result.is_err(), "expired token must be rejected");
    }

    #[test]
    fn wrong_secret_is_rejected() {
        let claims = valid_claims("00000000-0000-0000-0000-000000000001");
        let token =
            encode(&Header::default(), &claims, &EncodingKey::from_secret(SECRET)).unwrap();
        let wrong_secret = b"wrong-secret-32-chars-aaaaaaaaaa";
        let result = decode_jwt(&token, wrong_secret);
        assert!(result.is_err(), "token signed with wrong secret must be rejected");
    }

    #[test]
    fn garbage_token_is_rejected() {
        let result = decode_jwt("not.a.jwt", SECRET);
        assert!(result.is_err());
    }
}
