// JudicialPredict API Gateway — library root
//
// Exposes `build_app` and `RateLimitConfig` so integration tests can
// instantiate the router without binding a TCP listener.
// The binary (`src/main.rs`) calls `build_app` then binds a listener.

pub use crate::app::build_app;
pub use crate::rate_limit::RateLimitConfig;

pub(crate) mod app;
pub(crate) mod auth;
pub(crate) mod rate_limit;
