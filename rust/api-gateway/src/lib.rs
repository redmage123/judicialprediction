// JudicialPredict API Gateway — library root
//
// Exposes `build_app` so integration tests can instantiate the router without
// binding a TCP listener. The binary (`src/main.rs`) calls `build_app` then
// binds a listener.

pub use crate::app::build_app;

mod app;
mod auth;
