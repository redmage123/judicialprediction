// Imperative: connection-state management (ADR-FP-001 Tier 3).
// tokio + tungstenite WebSocket fan-out with Redis pub/sub backend.

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("event-broker placeholder — WebSocket fan-out + Redis pub/sub (Sprint 2)");
}
