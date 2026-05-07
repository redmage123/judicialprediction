# Auth Runbook — JudicialPredict API Gateway

JWT-based authentication is enforced on every `/graphql` request.
The `/health` endpoint is unauthenticated.

---

## Claim schema

Every request must carry an `Authorization: Bearer <token>` header whose
payload satisfies:

| Claim       | Type               | Required | Description |
|-------------|--------------------|----------|-------------|
| `sub`       | string             | yes      | Subject (user UUID or service account ID) |
| `tenant_id` | string (UUID v4)   | yes      | Tenant the token is scoped to; matched against Postgres RLS policy |
| `scopes`    | string[]           | no       | Permission scopes, e.g. `["features:read", "features:write"]` |
| `exp`       | UNIX epoch seconds | yes      | Expiry; tokens older than this are rejected |
| `iat`       | UNIX epoch seconds | yes      | Issued-at |
| `iss`       | string             | no (dev) | Issuer; required in prod (Sprint 3+) |
| `aud`       | string or string[] | no (dev) | Audience; required in prod (Sprint 3+) |

Example payload:
```json
{
  "sub": "usr_01HWXYZ",
  "tenant_id": "00000000-0000-0000-0000-000000000001",
  "scopes": ["features:read", "features:write"],
  "exp": 1746700000,
  "iat": 1746696400
}
```

---

## Local dev — issuing test tokens

### One-liner with `jwt-cli`

```bash
# Install once
cargo install jwt-cli

# Mint a 1-hour token for the dev tenant
jwt encode \
  --alg HS256 \
  --secret judicialpredict-test-jwt-secret! \
  --exp '+1h' \
  --payload 'sub=local-dev' \
  --payload 'tenant_id=00000000-0000-0000-0000-000000000001' \
  --payload 'scopes[]=features:read'
```

### Inline Rust helper (scripts/mint_dev_token.rs)

```rust
//! Run with: cargo script scripts/mint_dev_token.rs
//! (requires cargo-script or any single-file runner)
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct Claims {
    sub: &'static str,
    tenant_id: &'static str,
    scopes: Vec<&'static str>,
    exp: usize,
    iat: usize,
}

fn main() {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "judicialpredict-test-jwt-secret!".to_string());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;
    let claims = Claims {
        sub: "local-dev",
        tenant_id: "00000000-0000-0000-0000-000000000001",
        scopes: vec!["features:read"],
        exp: now + 3600,
        iat: now,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();
    println!("{token}");
}
```

### Using the token with curl

```bash
TOKEN=$(jwt encode --alg HS256 --secret judicialpredict-test-jwt-secret! \
  --exp '+1h' --payload 'sub=dev' \
  --payload 'tenant_id=00000000-0000-0000-0000-000000000001')

curl -s http://localhost:4000/graphql \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"{ healthcheck }"}'
```

---

## Environment variables

| Variable            | Required | Default              | Description |
|---------------------|----------|----------------------|-------------|
| `JWT_SECRET`        | **yes**  | *(none)*             | HS256 signing secret; minimum 32 bytes recommended |
| `FEATURE_STORE_GRPC_URL` | no | `http://127.0.0.1:4001` | Feature-store gRPC endpoint |

---

## Production plan (Sprint 3+)

Sprint 2 uses HS256 with a single shared secret stored in Kubernetes Secret
(created by External Secrets Operator pulling from HashiCorp Vault).

Sprint 3+ migration to ES256 (asymmetric, revocation-friendly):

1. Generate an EC P-256 key pair: `openssl ecparam -genkey -name prime256v1 -noout -out ec.key`
2. Expose the public JWKS endpoint from an internal auth service.
3. Replace `DecodingKey::from_secret` in `auth.rs` with `DecodingKey::from_ec_pem` or
   a cached JWKS fetcher.
4. Enable `iss` and `aud` validation in `Validation`.
5. Update External Secrets to provision the private key to the token issuer only;
   services only receive the public JWKS URL.

The `decode_jwt` function signature (`token: &str, secret: &[u8]`) stays the same
for HS256; for JWKS the call site passes the PEM-encoded public key bytes.

---

## Debugging 401 errors

| Symptom | Likely cause |
|---------|-------------|
| No `Authorization` header in request | Client forgot to attach token |
| `"Bearer "` prefix missing | Client set header as `Authorization: <token>` not `Bearer <token>` |
| Token valid but still 401 | `tenant_id` claim is not a valid UUID v4 |
| `ExpiredSignature` in logs | Token `exp` is in the past; mint a fresh token |
| `InvalidSignature` in logs | Token signed with wrong secret; check `JWT_SECRET` env var matches issuer |

Check api-gateway logs with:
```bash
journalctl -u jp-api-gateway -n 100 --no-pager
# or in docker-compose dev
docker logs judicialpredict_api_gateway
```
