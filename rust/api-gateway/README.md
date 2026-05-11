# api-gateway

Axum/GraphQL HTTP gateway for the JudicialPredict platform.

## Overview

Exposes a single GraphQL endpoint (`POST /graphql`) protected by:

1. **JWT authentication** — every request must carry `Authorization: Bearer <jwt>`.
2. **Per-tenant rate limiting** — token-bucket algorithm, configurable via env vars.

A `/health` endpoint is unauthenticated and returns `200 ok`.

## Rate Limiter

### Algorithm

Token-bucket (v2.14 spec §10). Pure math lives in the `rate-limit` crate
(ADR-FP-001: functional-core / imperative-shell). The imperative shell — the
DashMap store and Tower middleware — lives in `src/rate_limit.rs`.

Each tenant gets an independent bucket that:
- starts at full capacity (`requests_per_min` tokens).
- refills continuously at `requests_per_min / 60` tokens per second.
- is created lazily on the tenant's first request.

### Configuration

| Environment variable | Default | Description |
|---|---|---|
| `RATE_LIMIT_RPM` | `60` | Max requests per minute per tenant |
| `RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN` | `10` | Max GraphQL mutations per minute per tenant |

Example override:

```sh
RATE_LIMIT_RPM=120 RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN=20 ./api-gateway
```

### Response on exhaustion

```
HTTP/1.1 429 Too Many Requests
Retry-After: 1
Content-Type: text/plain

rate limit exceeded
```

`Retry-After` is the number of whole seconds until the next token is available
(RFC 9110 §10.2.4, rounded up).

### Swap path to Redis

The in-memory store (`MemoryStore`) is correct for a single replica. For
multi-replica production deployments, implement the `RateLimitStore` trait
backed by Redis atomic operations:

```rust
// src/rate_limit_redis.rs  (example skeleton)
use redis::Client;
use crate::rate_limit::{BoxFuture, RateLimitStore};
use rate_limit::Decision;
use uuid::Uuid;

pub struct RedisStore {
    client: Client,
    key_prefix: String,
    capacity: u32,
    refill_per_sec: f64,
}

impl RateLimitStore for RedisStore {
    fn check<'a>(&'a self, tenant_id: &'a Uuid, cost: u32) -> BoxFuture<'a, Decision> {
        Box::pin(async move {
            // Use a Lua EVAL script for atomic get-refill-decrement.
            // Return Decision::Allow or Decision::Deny { retry_after_ms }.
            todo!("Redis Lua EVAL implementation")
        })
    }
}
```

Pass `Arc<RedisStore>` to `build_app` wherever `Arc<dyn RateLimitStore>` is
expected — no other code changes required.

## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `JWT_SECRET` | yes | — | HS256 signing secret (raw bytes) |
| `FEATURE_STORE_GRPC_URL` | no | `http://127.0.0.1:4001` | Feature-store gRPC address |
| `RATE_LIMIT_RPM` | no | `60` | Max requests/min per tenant |
| `RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN` | no | `10` | Max mutations/min per tenant |

## Running locally

```sh
# Start the dev stack (Postgres + feature-store).
docker compose -f docker-compose.dev.yml up -d

JWT_SECRET=your-dev-secret cargo run -p api-gateway
```

## Tests

```sh
# Unit + integration tests (no docker stack required):
cargo test -p api-gateway

# Full E2E suite (requires docker-compose dev stack):
cargo test -p api-gateway --test e2e_smoke -- --include-ignored
```
