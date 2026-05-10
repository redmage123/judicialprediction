# CourtListener bulk-data ingest runbook

The `ingest-fetcher` binary downloads a CourtListener bulk dump for a single
court, parses it, and upserts opinions into `case_documents`.

## Manual run

```bash
# Build
cargo build -p ingest-fetcher --release

# Fetch + parse + upsert in one shot (requires DATABASE_URL).
DATABASE_URL=postgres://jp_app:secret@localhost:5432/jp \
  ./target/release/ingest-fetcher run tax

# Or break into steps for debugging:
./target/release/ingest-fetcher fetch tax
./target/release/ingest-fetcher parse /tmp/jp-ingest-tax.tar.gz
```

Subcommands:

- `fetch <court>` — downloads to `/tmp/jp-ingest-<court>.tar.gz`. No DB.
- `parse <path>` — parses a local tarball and prints `valid/skipped` counts. No DB.
- `run <court>` — fetch + parse + upsert. The typical ingest path.
- `run-rest <court> [--target N]` — REST API path (S3.6 fallback). Use when bulk-data is unreachable.

## Live-ingest reality (S3.6 findings, 2026-05-09)

CourtListener has **three rate-limit layers** stacked on the REST API. They
all apply, the tightest wins:

| Layer | Limit | Hint header / body |
|---|---|---|
| Per-minute | 5 / min on `/opinions/<id>/` and `/clusters/<id>/` | `429` body: `Rate limit exceeded: 5/min. Expected available in <N> seconds.` |
| Per-hour | observed ~30/hr after sustained use | same |
| Per-day | **125 / day across the whole REST API** (search + opinions + clusters all share the budget) | `429` body: `Rate limit exceeded: 125/day. Expected available in 73641 seconds.` |

The `/search/` endpoint has its own (more generous) limit and isn't subject
to the 125/day cap. Use it for ID enumeration; only hit `/opinions/<id>/`
for opinions you actually want to ingest.

Practical implications:
- **A single-day target of 1000+ opinions is impossible** at this quota.
- The Sprint-3 ingest landed only 4 tax-court opinions before hitting the
  daily cap. Each subsequent day, ingest can add up to 125 more.
- The bulk-data tarball path (which would deliver 13k tax opinions in one
  shot) is WAF-blocked at the Hetzner egress IP and remains broken even
  with auth. See "WAF block" below.

### Known incremental-upsert bug

The current `run-rest` path collects all opinions in memory and upserts at
the end. If the daily cap fires mid-fetch, the in-memory opinions are
lost. **Sprint-4 follow-up: switch to per-opinion upsert** so a partial
run still persists what it got.

### WAF block on bulk-data

`https://www.courtlistener.com/api/bulk-data/opinions/<court>.tar.gz` returns
HTTP 403 from CloudFront for Hetzner Frankfurt egress IPs (`78.47.x.x`),
even with an `Authorization: Token <api-token>` header. The block is IP-based.
To unblock:
1. Email Free Law Project (`https://free.law/contact/`) requesting Hetzner-IP
   allowlist for non-profit / research access.
2. OR run ingest from a non-datacenter IP (residential, AWS).
3. OR subscribe to their paid commercial API.

## Production schedule

Daily cron at 04:00 UTC; court rotation by day-of-week (S4.11):

| DOW    | Court  | Rationale                                                   |
|--------|--------|-------------------------------------------------------------|
| Mon    | tax    | tax has 13k opinions; biggest pool, hit it twice/week       |
| Tue    | cafc   | US Federal Circuit (patent + IP cases — distinct posture)   |
| Wed    | bia    | Board of Immigration Appeals                                |
| Thu    | tax    | second tax day                                              |
| Fri    | scotus | Supreme Court                                               |
| Sat/Sun| tax    | absorb weekend slots into the largest pool                  |

```cron
0 4 * * * /opt/ai-elevate/gigforge/projects/judicialpredict/scripts/courtlistener-daily.sh
```

Override the auto-selected court for a manual run: `COURT=scotus ./scripts/courtlistener-daily.sh`.

**Sprint-5 follow-up:** parallel multi-court if FLP allowlist drops the daily cap.

## Expected timings

| Court  | Compressed | Uncompressed | Opinions | Wall-clock target |
|--------|-----------:|-------------:|---------:|------------------:|
| tax    |     ~30 MB |      ~150 MB |    ~10 k | < 10 min          |
| ny     |    ~120 MB |      ~600 MB |    ~40 k | < 25 min          |
| scotus |    ~200 MB |        ~1 GB |    ~28 k | < 30 min          |

## Failure modes

- **Network: rate-limited / 5xx.** `reqwest::get` returns an error; the binary
  exits non-zero. Re-run after backoff. CourtListener does not currently
  rate-limit bulk dumps but reserves the right to.
- **Partial tarball.** Treated as a parse error per malformed entry; the run
  continues and reports `skipped` count. Re-run to pick up missed entries.
- **Postgres disk budget.** A single court adds ≤ 1 GB to `case_documents`.
  Monitor `pg_size` and alert at > 80 GB pre-vacuum.
- **Idempotency.** `ON CONFLICT (opinion_id) DO UPDATE` lets re-runs land safely.

## RLS posture

`case_documents` has **no RLS**. Opinions are public records and the table is
shared across all tenants. This is documented in the table comment and in
`docs/architecture/tenant-settings.md`. Any future tenant-private case-document
table belongs in a separate table with full RLS.

## Sprint-3 follow-ups

- Real-network smoke test: a CI job that fetches the smallest real dump
  weekly and asserts the row count is in a sane range.
- Multi-court fan-out: a single command that ingests N courts in parallel with
  a shared Postgres connection pool.
- S3 mirror: optionally upload each downloaded `.tar.gz` to s3://<bucket>/raw/
  before parsing, so we have an immutable record of what we ingested.
- Real chunked streaming on the fetch path (currently buffers full body in
  memory; fine for 30 MB, not for 200 MB).
- Per-row error-isolation upsert: today one failing row aborts the whole
  upsert; switch to a per-row INSERT with isolated error counters.
