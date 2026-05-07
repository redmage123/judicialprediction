# Dev Stack Runbook — JudicialPredict

Local development compose stack. **For local dev and CI integration tests only.**
Production uses operators per ADR-003 / spec §11.5.

---

## Lifecycle commands

```bash
# Start all services (detached)
docker compose -f docker-compose.dev.yml up -d

# Wait for all healthchecks to pass before running tests
docker compose -f docker-compose.dev.yml up -d --wait

# Stop (preserve volumes)
docker compose -f docker-compose.dev.yml down

# Full reset — destroys all data volumes
docker compose -f docker-compose.dev.yml down -v

# View logs
docker compose -f docker-compose.dev.yml logs -f [service]

# Check healthcheck status
docker compose -f docker-compose.dev.yml ps
```

---

## Services

### Postgres 16 + pgvector

| Property | Value |
|----------|-------|
| Image | `pgvector/pgvector:pg16` |
| Host (local) | `127.0.0.1:5454` |
| Host (compose network) | `judicialpredict_postgres:5432` |
| Database | `judicialpredict_dev` |
| User | `judicialpredict` |
| Password | `judicialpredict_dev_pwd` |

**Connection string:**
```
postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev
```

**For sqlx (Rust):**
```bash
export DATABASE_URL=postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev
```

**CLI:**
```bash
psql postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev

# Verify pgvector extension is available
psql ... -c "SELECT * FROM pg_available_extensions WHERE name = 'vector';"

# Enable pgvector in the dev DB (run once after first start)
psql ... -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

**Healthcheck:** `pg_isready -U judicialpredict -d judicialpredict_dev`

---

### Neo4j 5 Community

| Property | Value |
|----------|-------|
| Image | `neo4j:5-community` |
| Browser (local) | `http://127.0.0.1:7474` |
| Bolt (local) | `bolt://127.0.0.1:7687` |
| Bolt (compose network) | `bolt://judicialpredict_neo4j:7687` |
| User | `neo4j` |
| Password | `judicialpredict_dev_pwd` |

**CLI:**
```bash
# Interactive Cypher shell
cypher-shell -a bolt://127.0.0.1:7687 -u neo4j -p judicialpredict_dev_pwd

# One-shot query
cypher-shell -a bolt://127.0.0.1:7687 -u neo4j -p judicialpredict_dev_pwd \
  'MATCH (n) RETURN count(n) AS node_count'
```

**Browser:** open `http://127.0.0.1:7474` in a browser; connect with `bolt://localhost:7687`, user `neo4j`, password `judicialpredict_dev_pwd`.

**Healthcheck:** `cypher-shell 'RETURN 1'` — takes up to 30s on first start.

---

### Redis 7

| Property | Value |
|----------|-------|
| Image | `redis:7-alpine` |
| Host (local) | `127.0.0.1:6385` |
| Host (compose network) | `judicialpredict_redis:6379` |
| Auth | none (dev only) |

**Connection string (Python):**
```python
redis.Redis(host="127.0.0.1", port=6385, decode_responses=True)
```

**Connection string (Rust / fred / redis-rs):**
```
redis://127.0.0.1:6385
```

**CLI:**
```bash
redis-cli -h 127.0.0.1 -p 6385 ping          # → PONG
redis-cli -h 127.0.0.1 -p 6385 info server   # server info
redis-cli -h 127.0.0.1 -p 6385 xlen <stream> # check stream length
```

**Persistence:** AOF (`appendonly yes`) + RDB snapshots. Data survives container restarts; destroyed by `down -v`.

**Healthcheck:** `redis-cli ping`

---

### MinIO

| Property | Value |
|----------|-------|
| Image | `minio/minio:latest` |
| S3 API (local) | `http://127.0.0.1:9100` |
| S3 API (compose network) | `http://judicialpredict_minio:9000` |
| Web console | `http://127.0.0.1:9101` |
| Access key | `judicialpredict` |
| Secret key | `judicialpredict_dev_pwd` |

**CLI (mc):**
```bash
# Install mc if not present
curl -sSL https://dl.min.io/client/mc/release/linux-amd64/mc -o /usr/local/bin/mc
chmod +x /usr/local/bin/mc

# Configure alias
mc alias set jpdev http://127.0.0.1:9100 judicialpredict judicialpredict_dev_pwd

# Create a bucket
mc mb jpdev/jp-documents

# List buckets
mc ls jpdev
```

**AWS SDK / boto3 (Python):**
```python
import boto3
s3 = boto3.client(
    "s3",
    endpoint_url="http://127.0.0.1:9100",
    aws_access_key_id="judicialpredict",
    aws_secret_access_key="judicialpredict_dev_pwd",
)
```

**Rust (aws-sdk-s3):**
```rust
// Set endpoint override to http://127.0.0.1:9100
// AWS_ACCESS_KEY_ID=judicialpredict
// AWS_SECRET_ACCESS_KEY=judicialpredict_dev_pwd
```

**Healthcheck:** `curl -sf http://localhost:9000/minio/health/live`

---

## CI integration tests

### Running on the host (default)

Services are exposed on `127.0.0.1` with non-standard ports. Use the connection strings above.

```bash
# Start stack and wait for all healthchecks
docker compose -f docker-compose.dev.yml up -d --wait

# Run integration tests (example)
DATABASE_URL=postgresql://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev \
REDIS_URL=redis://127.0.0.1:6385 \
cargo test --workspace -- --include-ignored integration
```

### Running inside a container on jp_dev_net

Use container names as hostnames and internal ports:

| Service | Address inside jp_dev_net |
|---------|--------------------------|
| Postgres | `judicialpredict_postgres:5432` |
| Neo4j Bolt | `judicialpredict_neo4j:7687` |
| Redis | `judicialpredict_redis:6379` |
| MinIO S3 | `judicialpredict_minio:9000` |

Add the test container to `jp_dev_net`:
```yaml
# docker-compose.dev.yml snippet for a test runner container
services:
  test-runner:
    build: .
    networks:
      - jp_dev_net
    environment:
      DATABASE_URL: postgresql://judicialpredict:judicialpredict_dev_pwd@judicialpredict_postgres:5432/judicialpredict_dev
      REDIS_URL: redis://judicialpredict_redis:6379
```

### Which tests assume which service

| Test suite / crate | Required services |
|--------------------|------------------|
| `feature-store` integration tests | Postgres |
| `event-broker` integration tests | Redis |
| `ingest-fetcher` integration tests | Postgres, MinIO |
| `api-gateway` integration tests | Postgres, Redis |
| Graph-ML / knowledge-graph tests (Python, Sprint M3+) | Neo4j |
| Document-ingestion tests (Python, Sprint M2+) | MinIO |

Integration tests that require a live service should be gated with `#[ignore]` in Rust (run with `cargo test -- --include-ignored`) or `pytest -m integration` in Python, so the default `cargo test` / `pytest` run remains fast and infrastructure-free.

---

## Troubleshooting

**Neo4j takes too long to start:** the healthcheck allows 30s start_period. If it still fails, check memory — Neo4j needs at least 1 GB free. Reduce heap in the compose file if constrained.

**pgvector extension missing:** the `pgvector/pgvector:pg16` image ships the extension binary but does not auto-enable it. Run `CREATE EXTENSION IF NOT EXISTS vector;` once after the first `up`.

**Port conflicts:** all services bind to `127.0.0.1` only (not `0.0.0.0`). If a port is in use, change the left-hand side of the port mapping (e.g. `"127.0.0.1:5455:5432"`).

**Volume data corruption:** run `docker compose -f docker-compose.dev.yml down -v` to destroy and recreate all volumes from scratch.
