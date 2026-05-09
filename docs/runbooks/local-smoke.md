# Local end-to-end smoke

This runbook brings up the demo vertical slice on a single host (Hetzner
or a dev box) and walks through the smoke checks that prove the polyglot
path is intact.

## Stack

| Service          | Port      | Source                              |
|------------------|-----------|-------------------------------------|
| Postgres         | 5454      | `docker-compose.dev.yml`            |
| ml-inference-svc | 8001      | `python/ml-inference-svc`           |
| api-gateway      | 4040      | `rust/api-gateway` (release binary) |
| Next.js web      | 3030      | `web/`                              |

> Port note: `4000` and `3000` are commonly taken on AI-Elevate hosts by
> other services (`job-board-api` and `course-creator-frontend`). Use
> `GATEWAY_BIND=0.0.0.0:4040` and `next start -p 3030` to dodge collisions.

## Prereqs

- Postgres up and `_sqlx_migrations` includes `20260509120000` and
  `20260509130000` (tenant_settings + case_documents).
- `cargo build --release -p api-gateway` succeeded.
- `npm run build` succeeded inside `web/`.

## Bring-up

```bash
export JWT_SECRET="dev-only-NOT-A-REAL-SECRET-1234567890abcdef"
export DATABASE_URL="postgres://judicialpredict:judicialpredict_dev_pwd@127.0.0.1:5454/judicialpredict_dev"
export AUDIT_DATABASE_URL="$DATABASE_URL"
export ML_INFERENCE_URL="http://127.0.0.1:8001"
export RATE_LIMIT_RPM=600
export RATE_LIMIT_GRAPHQL_MUTATIONS_PER_MIN=600
export GATEWAY_BIND="0.0.0.0:4040"

# 1. Python ML
( cd python/ml-inference-svc && nohup uv run python -m ml_inference_svc.main > /tmp/jp-ml.log 2>&1 < /dev/null & )

# 2. Rust gateway
nohup ./rust/target/release/api-gateway > /tmp/jp-gw.log 2>&1 < /dev/null &

# 3. Next.js (point the BFF at the gateway port)
cat > web/.env.local <<EOF
JWT_DEV_SECRET=$JWT_SECRET
GATEWAY_INTERNAL_URL=http://127.0.0.1:4040
EOF
( cd web && nohup npx next start -p 3030 > /tmp/jp-web.log 2>&1 < /dev/null & )
```

Wait for `/login`, `/health`, and `/healthz` to all return 200.

## Smoke checks (the demo DoD)

```bash
# 1. Login mints a cookie.
COOKIE_JAR=/tmp/jp-cookies; rm -f $COOKIE_JAR
curl -sf -c $COOKIE_JAR -X POST http://127.0.0.1:3030/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"dev@example.test","password":"dev-pass"}'

# 2. Authenticated /case/new returns 200.
curl -sf -b $COOKIE_JAR -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3030/case/new

# 3. Mutation through the BFF reaches the Python predictor.
curl -s -b $COOKIE_JAR -X POST http://127.0.0.1:3030/api/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"mutation P($input: PredictInput!){ predictCaseOutcome(input: $input) { pWin ciLower ciUpper coverage modelVersion predictedAtUnix } }","variables":{"input":{"judgeSeverity":0.65,"attorneyWinRate":0.72,"ideologyDistance":0.41,"materialityScore":0.88,"proceduralMotionCount":3,"caseType":"civil","jurisdiction":"us-federal"}}}'

# 4. Audit row landed.
docker exec judicialpredict_postgres psql -U judicialpredict -d judicialpredict_dev -tA \
  -c "SELECT COUNT(*) FROM audit_log WHERE action='predict.invoke' AND ts > NOW() - INTERVAL '1 minute';"
```

A scripted version of all 10 smoke checks lives at
`/tmp/jp_smoke_e2e.sh` while iterating; promote it to
`scripts/smoke-e2e.sh` once stable.

## Common gotchas

- **Cannot POST /graphql (HTML error page)** — the BFF fell back to its
  default `localhost:4000` because `GATEWAY_INTERNAL_URL` was unset, and
  hit a different service's Express app. Set the env var.
- **`audit_log.created_at` does not exist** — the column is `ts`, not
  `created_at`.
- **Address already in use on port 4000** — another container owns it on
  Hetzner. Override with `GATEWAY_BIND=0.0.0.0:4040`.
- **Port 3000 collision** — course-creator's frontend is there. Use
  `next start -p 3030`.
- **JWT signature mismatch between Next.js and api-gateway** — both must
  use the same secret. The web side reads `JWT_DEV_SECRET`, the gateway
  reads `JWT_SECRET`. Set both to the same value end-to-end.

## Sprint-3 follow-ups

- Wrap the bring-up in a `docker-compose.smoke.yml` so a single
  `docker compose up` brings the whole stack up.
- Ship `scripts/smoke-e2e.sh` as a real CI gate (today it's an ad-hoc
  shell).
- Real CourtListener-trained model artefact (S3.7) replaces the synthetic
  one currently served by ml-inference-svc.
