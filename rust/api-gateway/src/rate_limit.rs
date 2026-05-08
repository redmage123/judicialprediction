//! Per-tenant rate-limiting middleware and store abstraction.
//!
//! Architecture (ADR-FP-001 — functional-core / imperative-shell):
//!
//! - Pure token-bucket math lives in the `rate-limit` crate.
//! - This module is the **imperative shell**: it holds mutable bucket state,
//!   makes async decisions, and feeds the result to the Tower middleware.
//!
//! ## Execution order in the request pipeline
//!
//! ```text
//! HTTP request
//!   │
//!   ▼
//! jwt_middleware          ← outermost route_layer (applied last)
//!   │  • parses Authorization: Bearer <jwt>
//!   │  • injects TenantId + Claims into request extensions
//!   │  • returns 401 on any auth failure (short-circuits)
//!   ▼
//! rate_limit_middleware   ← inner route_layer (applied first)
//!   │  • reads TenantId from extensions
//!   │  • consumes 1 token from the per-tenant bucket
//!   │  • returns 429 + Retry-After on Deny
//!   ▼
//! graphql_handler
//! ```
//!
//! ## Swap path to Redis
//!
//! Implement [`RateLimitStore`] for a Redis-backed struct:
//!
//! ```ignore
//! struct RedisStore { client: redis::Client, key_prefix: String }
//!
//! impl RateLimitStore for RedisStore {
//!     fn check<'a>(&'a self, tenant_id: &'a Uuid, cost: u32) -> BoxFuture<'a, Decision> {
//!         Box::pin(async move {
//!             // Use Lua EVAL for atomic get-refill-decrement on Redis.
//!             // ...
//!         })
//!     }
//! }
//! ```
//!
//! Pass a `Arc<RedisStore>` wherever `Arc<dyn RateLimitStore>` is accepted.

use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use rate_limit::{check, Decision, TokenBucket};
use uuid::Uuid;

use crate::app::TenantId;

// ---------------------------------------------------------------------------
// Config (public — threaded through main.rs)
// ---------------------------------------------------------------------------

/// Rate-limiting parameters read from env vars and passed to [`crate::app::build_app`].
#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    /// Max requests per minute per tenant.  Env: `RATE_LIMIT_RPM` (default 60).
    pub requests_per_min: u32,
    /// Max GraphQL mutations per minute per tenant.
    /// Env: `RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN` (default 10).
    pub mutations_per_min: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self { requests_per_min: 60, mutations_per_min: 10 }
    }
}

// ---------------------------------------------------------------------------
// RateLimitStore trait
// ---------------------------------------------------------------------------

/// Type alias for the boxed future returned by [`RateLimitStore::check`].
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Abstraction over the token-bucket store.
///
/// The in-memory [`MemoryStore`] is used in dev/test; a Redis-backed impl
/// (see module docs) should be used in production for multi-replica deploys.
pub trait RateLimitStore: Send + Sync + 'static {
    /// Consume `cost` tokens for `tenant_id`.
    ///
    /// Returns [`Decision::Allow`] when tokens are available,
    /// or [`Decision::Deny`] with a back-off hint.
    fn check<'a>(&'a self, tenant_id: &'a Uuid, cost: u32) -> BoxFuture<'a, Decision>;
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

/// In-process token-bucket store backed by a [`DashMap`].
///
/// Each tenant gets a lazily-created [`TokenBucket`] wrapped in a [`Mutex`].
/// The mutex is held only for the duration of the pure [`check`] call
/// (nanoseconds), so there is no meaningful contention.
pub struct MemoryStore {
    buckets: Arc<DashMap<Uuid, Mutex<TokenBucket>>>,
    capacity: u32,
    refill_per_sec: f64,
}

impl MemoryStore {
    /// Create a store where every tenant bucket has the given `requests_per_min`
    /// capacity and an equivalent per-second refill rate.
    pub fn new(requests_per_min: u32) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            capacity: requests_per_min,
            refill_per_sec: requests_per_min as f64 / 60.0,
        }
    }
}

impl RateLimitStore for MemoryStore {
    fn check<'a>(&'a self, tenant_id: &'a Uuid, cost: u32) -> BoxFuture<'a, Decision> {
        Box::pin(async move {
            let now = Instant::now();
            // Lazily insert a new full bucket for first-seen tenants.
            let entry = self.buckets.entry(*tenant_id).or_insert_with(|| {
                Mutex::new(TokenBucket::new(self.capacity, self.refill_per_sec))
            });
            // Lock held only for the pure check() call.
            let mut bucket = entry.lock().expect("bucket mutex poisoned");
            check(&mut bucket, now, cost)
        })
    }
}

// ---------------------------------------------------------------------------
// JWT axum middleware (runs first / outermost)
// ---------------------------------------------------------------------------

/// Decode and verify the `Authorization: Bearer <jwt>` header, then inject
/// [`TenantId`] and [`crate::auth::Claims`] into request extensions.
///
/// Returns HTTP 401 on any auth failure; the rate-limit and GraphQL layers
/// are never reached for unauthenticated requests.
pub async fn jwt_middleware(
    axum::extract::State(state): axum::extract::State<Arc<crate::app::AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = crate::auth::decode_jwt(token, &state.jwt_secret)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let tenant_id =
        Uuid::parse_str(&claims.tenant_id).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Inject into extensions so downstream layers/handlers can read without
    // re-parsing the JWT.
    req.extensions_mut().insert(TenantId(tenant_id));
    req.extensions_mut().insert(claims);

    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// Rate-limit axum middleware (runs after jwt_middleware)
// ---------------------------------------------------------------------------

/// Consume 1 token from the per-tenant bucket; return HTTP 429 on exhaustion.
///
/// Reads [`TenantId`] from request extensions (set by [`jwt_middleware`]).
/// If the extension is absent (should not happen with correct layer wiring),
/// the request is passed through unchanged — the JWT layer would already have
/// rejected it.
///
/// On denial, the response includes a `Retry-After` header (RFC 9110 §10.2.4)
/// with the number of seconds to wait, rounded up.
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<Arc<crate::app::AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let tenant_id = req.extensions().get::<TenantId>().copied();

    let Some(TenantId(id)) = tenant_id else {
        // TenantId not in extensions — JWT layer will have short-circuited; pass through.
        return next.run(req).await;
    };

    let decision = state.rate_store.check(&id, 1).await;

    match decision {
        Decision::Allow => next.run(req).await,
        Decision::Deny { retry_after_ms } => {
            // Retry-After is specified in whole seconds (RFC 9110 §10.2.4).
            let retry_secs = retry_after_ms.div_ceil(1_000);
            Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", retry_secs.to_string())
                .body(Body::from("rate limit exceeded\n"))
                .expect("static 429 response is always valid")
        }
    }
}
