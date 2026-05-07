# Build context: /opt/ai-elevate/gigforge/projects/judicialpredict/
# (the monorepo root, so both rust/ and protos/ are available)
#
# docker build -t jp-api-gateway:dev \
#   -f rust/api-gateway/Dockerfile \
#   /opt/ai-elevate/gigforge/projects/judicialpredict/

# ── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:latest AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy proto sources first (needed by feature-store/build.rs at compile time).
COPY protos/ /protos/

# Copy the entire Rust workspace.
COPY rust/ .

# Build only the api-gateway binary in release mode.
RUN cargo build --release -p api-gateway

# ── Stage 2: minimal runtime image ────────────────────────────────────────────
FROM debian:bookworm-slim AS runner

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 appgroup \
    && useradd --uid 10001 --gid appgroup --no-create-home appuser

COPY --from=builder /build/target/release/api-gateway /usr/local/bin/api-gateway

USER appuser

EXPOSE 4000

CMD ["/usr/local/bin/api-gateway"]
