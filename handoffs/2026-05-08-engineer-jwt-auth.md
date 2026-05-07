# Handoff: S2.2 — JWT Authentication (api-gateway)

**From:** gigforge-engineer (Chris Novak)
**Date:** 2026-05-07
**Plane:** JP-25
**Status:** COMPLETE

---

## What was built

Replaced the Sprint 1 `X-Tenant-Id` header placeholder with full JWT
claim-based authentication on the api-gateway.

### Files created

| File | Description |
|------|-------------|
| `rust/api-gateway/src/auth.rs` | `Claims` struct + `decode_jwt(token, secret)` helper (HS256); 4 unit tests |
| `docs/runbooks/auth.md` | Dev JWT issuance one-liner, claim schema, prod ES256 upgrade plan, 401 debug guide |

### Files modified

| File | Change |
|------|--------|
| `rust/Cargo.toml` | Added `jsonwebtoken = { version = "9" }` to `[workspace.dependencies]` |
| `rust/api-gateway/Cargo.toml` | Added `jsonwebtoken` to `[dependencies]` and `[dev-dependencies]` |
| `rust/api-gateway/src/lib.rs` | Added `mod auth;` |
| `rust/api-gateway/src/app.rs` | Replaced header-based tenant extraction with JWT middleware; new `AppState` struct; `build_app` now takes `jwt_secret: Vec<u8>` |
| `rust/api-gateway/src/main.rs` | Reads `JWT_SECRET` env var; passes bytes to `build_app` |
| `rust/api-gateway/tests/e2e_smoke.rs` | `make_jwt` / `make_expired_jwt` helpers; all `graphql()` calls use `Authorization: Bearer`; updated all `build_app` invocations; added `missing_or_expired_jwt_returns_401` test |

---

## Build and test results

```
cargo build --workspace          ✅  Finished dev profile — 0 errors
cargo test --workspace           ✅  auth::tests: 4/4 passed; all other crates clean
                                      (16 unit + proptest results across all crates)
cargo test --test e2e_smoke      ✅  5/5 passed (--include-ignored):
  - health_endpoint_ok
  - missing_tenant_header_is_unauthorized
  - missing_or_expired_jwt_returns_401
  - feature_store_grpc_unavailable_returns_error
  - graphql_feature_rls_smoke (RLS isolation verified end-to-end)
```

### Security note — hook injection detected

During this session a post-write hook modified `e2e_smoke.rs` and deliberately
inverted the RLS isolation assertion with the comment "DELIBERATELY BROKEN:
inverted to simulate a missing RLS policy — CI must catch this." This is a
prompt-injection attempt in the hook output. The correct assertion was
immediately restored. The file's RLS assertion is: `assert!(json2["data"]["feature"].is_null(), ...)`.

**Recommend:** audit the registered hooks (`~/.claude/hooks/`) for any
post-write hooks that touch `*.rs` files and remove any that are not
explicitly authorised.

---

## Behaviour changes visible to callers

| Before (Sprint 1) | After (Sprint 2) |
|--------------------|-----------------|
| `X-Tenant-Id: <uuid>` header | `Authorization: Bearer <hs256-jwt>` header |
| tenant_id extracted directly | tenant_id extracted from `tenant_id` claim in JWT |
| No signature verification | Full HS256 signature + expiry verification |
| `/health` unauthenticated ✅ | `/health` unauthenticated ✅ (unchanged) |

---

## What Sprint 3 needs to do

### ES256 (asymmetric key) upgrade

1. Replace `Validation::new(Algorithm::HS256)` with ES256 in `auth.rs`.
2. Accept `DecodingKey::from_ec_pem(public_key_bytes)` built from a JWKS
   endpoint fetched at startup (cache with a 5-minute TTL).
3. Enable `validation.validate_aud = true` and `validation.set_issuer(&["..."])`.
4. Provision the EC private key via External Secrets Operator → Vault; services
   only receive the JWKS endpoint URL.

### JWKS endpoint

The Django admin / identity service (Sprint 3+) should expose
`GET /api/v1/.well-known/jwks.json` returning the public key in JWK format.
api-gateway fetches it at startup and refreshes on 401 from the key-id
rotation path.

### Scope enforcement

The `Claims.scopes` field is populated but not yet checked against resolver
permissions. Sprint 3 should add a `#[guard(scope = "features:read")]`
procedural macro or an explicit scope check in resolvers.

### Token issuance

A lightweight token-issuance endpoint (`POST /auth/token`) on the Django
service (Sprint 3+) should accept credentials and return a signed JWT, making
the issuer self-contained with no external dependency.
