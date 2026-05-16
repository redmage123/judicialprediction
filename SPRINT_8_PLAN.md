# Sprint 8 — JudicialPredict

**Theme:** Second Tier-A ideology source lands. Martin-Quinn judicial ideal
points (time-varying, per-term) flow alongside Bonica DIME. The intake form
and compliance disclosure carry both. Schema gains a temporal dimension that
Sprint 9 (JCS) will reuse, and the matcher / name-norm helpers extracted in
Sprint 7 are now shared across two ingest crates.

**Window:** 2026-06-08 → 2026-06-29 (3 weeks).

---

## Carry-forward from Sprint 7

None blocking. Sprint 7 shipped:

* Bonica DIME ingest (`rust/dime-ingest/`) + matcher (three-pass, no fuzzy),
* `bio.dime` provenance schema + GIN index,
* extractFeatures returns `ideologyDistance` + `ideologySource = bonica_dime`,
* `BONICA DIME -X.XX` badge on the intake form,
* "Tier-A sources used" compliance footer on case detail,
* end-to-end smoke script.

The Sprint-8 work below mostly **mirrors** that pattern. Where it diverges
is called out explicitly.

---

## Goals

1. **Martin-Quinn ingest** lives — a Rust crate reads the public MQ scores
   CSV, groups by judge, writes `bio.mqs.scores[]` (full per-term series)
   plus `latest_score` / `latest_term` shortcuts.
2. **Source precedence** — when both DIME and MQ are available for a
   judge, MQ wins (MQ is voting-record-derived; DIME is a campaign-finance
   proxy). When only one is present, that one wins. UI shows whichever
   source actually fired.
3. **Compliance footer enumerates both sources** — the per-prediction memo
   now names every Tier-A source actually used.
4. **Pattern stays reusable.** A third ingest (JCS in Sprint 9) should
   need only a CSV reader + bio-key — the matcher and provenance plumbing
   are now shared.

What we are **not** doing in Sprint 8:

* Date-aware MQ score lookup. The model takes a single scalar
  `ideologyDistance`; we resolve to the judge's most recent term snapshot
  and stamp the term in provenance. Per-case-date lookup arrives only when
  the model takes a date feature or the resolver is given one explicitly
  — Sprint 9 or 10.
* Retraining on real ideology features. Still waiting on >=3 sources.
* Federal-circuit + district extension of MQ via Epstein-Martin-Quinn-Segal
  (the JCS scaling step). That's Sprint 9.
* Per-case provenance persistence. Sprint 9 adds the column; right now the
  compliance footer describes the source the gateway will draw on, not
  the source-of-record at prediction time.

---

## Background — what Martin-Quinn actually is

Martin-Quinn scores (Andrew Martin + Kevin Quinn, 2002) place each US
Supreme Court justice on a one-dimensional ideal-point scale for each
court term they served. The scale is estimated jointly from voting
records via a dynamic Bayesian ideal-point model; negative = liberal,
positive = conservative. The publicly released file
`https://mqscores.lsa.umich.edu/measures.php` is updated annually after
each completed Term.

Rows have the form `(justice, term, post_mean, post_sd, post_med, ...)`,
one per justice-term pair. ~36 justices, 60+ terms each, total ~2300
rows. Trivially small CSV; no chunking needed.

Why this second (after DIME):

* Voting-record-based — closer to "how the judge actually rules" than
  campaign-finance is. Stronger defensible for in-courtroom use.
* Provides a TEMPORAL dimension the JCS step needs.
* Same name-matching infrastructure ports straight over.

Why this is harder than DIME:

* Time-varying. Each justice has 30+ rows. The schema has to keep the
  series, not just a scalar.
* Coverage limited to SCOTUS. JCS extends this to other federal courts in
  Sprint 9 by joint-scaling with Common Space scores.

---

## Tickets

### S8.1 — Plan doc (this file)

**Owner:** dev
**Acceptance:** `SPRINT_8_PLAN.md` committed.

### S8.2 — Schema doc-only migration: `bio.mqs`

**Owner:** dev-backend
**Acceptance:**

* New SQL migration `2026MMDDHHMMSS_mqs_provenance.sql`. Like the S7.2
  migration, this is documentation + index: the GIN index on `judges.bio`
  added in S7.2 already covers `bio ? 'mqs'`, so no new index. We update
  the `COMMENT ON COLUMN judges.bio` to describe the `bio.mqs` shape.
* Documented JSONB shape:
  ```json
  {
    "mqs": {
      "scores": [
        { "term": 1990, "post_mean": -0.41, "post_sd": 0.12 },
        { "term": 1991, "post_mean": -0.38, "post_sd": 0.11 }
      ],
      "latest_score": -0.38,
      "latest_term":  1991,
      "release":      "mqs-2023-v1",
      "source_id":    "<mq-justice-id>",
      "ingested_at":  "2026-06-09T10:00:00Z",
      "match_confidence": "exact"
    }
  }
  ```

### S8.3 — Rust crate: `mqs-ingest`

**Owner:** dev-backend
**Acceptance:**

* `rust/mqs-ingest/` workspace member, binary `mqs-ingest`.
* Subcommand `ingest --csv <path> --tenant-id <uuid> [--report <path>]
  [--release <tag>] [--dry-run]`.
* CSV columns understood: `justice_name`, `term`, `post_mean`, `post_sd`,
  `justiceID` (Bonica/Spaeth's stable ID); extra columns ignored.
* Aggregation step before write: rows grouped by `justiceID`, sorted by
  term ascending, the highest-term row provides `latest_score` and
  `latest_term`. Whole series written under `scores`.
* Matcher reuses `dime_ingest::matcher::match_row` patterns — same
  three-pass strategy, same confidence enum. We extract `match_row` into
  a small shared helper so the next ingest doesn't fork it.
* Idempotent. Re-running rebuilds the same `bio.mqs` payload byte-for-byte.

### S8.4 — Fixture + tests

**Owner:** dev-backend
**Acceptance:**

* `rust/mqs-ingest/fixtures/mqs-mini.csv` — 25 rows shaped like the real
  MQ release. Covers 4 justices x 5-7 terms each plus one row with a
  malformed `post_mean` and one orphan term to exercise edge cases.
* `cargo test -p mqs-ingest` covers:
  * CSV header detection.
  * Aggregation by justiceID picks the highest-term row for
    `latest_score`.
  * Malformed `post_mean` rows skipped, not zeroed.
  * Same name-match edge cases as DIME (single token, court fallback,
    ambiguous duplicates).

### S8.5 — Feature pipeline reads MQ

**Owner:** dev-backend
**Acceptance:**

* `extract_features_from_text` lookup query gains two columns:
  `bio->'mqs'->>'latest_score'` and `bio->'mqs'->>'latest_term'`.
* When `bio.mqs.latest_score` is present, it overrides `bio.dime.cfscore`
  for the purposes of `ideologyDistance`. The `ideologySource` enum
  variant becomes `martin_quinn`. The `ideologyRelease` carries the
  Sprint-8 release tag (e.g. `mqs-2023-v1`).
* Three new fields surface so the UI can render the temporal hint:
  `ideologyTerm` (the year MQ used), `ideologyRawScore` (the raw
  `post_mean` in MQ's native scale, used for the chip tooltip).
* Existing DIME path continues to fire as a fallback when MQ is absent —
  zero regression in S7's smoke.

### S8.6 — UI: MQ badge + compliance update

**Owner:** dev-frontend
**Acceptance:**

* Intake form: when prefill came from MQ, render a green
  **MARTIN-QUINN YYYY** chip with the raw score in the tooltip. DIME chip
  stays unchanged for the DIME-only path; never both shown — only the
  source that actually fed `ideologyDistance`.
* Case detail compliance footer: the Tier-A sources list now describes
  both DIME and MQ, with the precedence rule ("MQ preferred when both
  available; cf. voting-record vs campaign-finance methodology").

### S8.7 — Smoke + commit + push

**Owner:** dev / QA
**Acceptance:**

* `scripts/mqs-smoke.sh` mirrors `dime-smoke.sh`: ingest the fixture,
  assert at least one judge gets `bio.mqs.latest_score`, hit
  `extractFeatures` with the matched judge's name, verify the response
  carries `ideologySource=martin_quinn` and a non-null `ideologyTerm`.
* `cargo test --workspace` clean for new + existing crates.
* Push to `origin/main` after a fast-forward sync.

---

## Out of scope (deferred to Sprint 9+)

* **S9.x — JCS (Judicial Common Space).** Same temporal shape as MQ but
  extends coverage to circuit + district judges via Epstein et al.'s
  joint-scaling step. Reuses S8 schema verbatim.
* **S9.x — Per-case provenance persistence.** Add `cases.ideology_source` +
  `cases.ideology_release` columns so the printed memo names the EXACT
  source used at prediction time, not the source available at memo time.
* **S9.x — Attorney ideology.** Needs `attorneys` table + state-bar + FEC
  contribution name-matching. Still gated on the bar-roll bulk ingest.
* **S10.x — HEXACO / MFT personality.** Needs the
  `gemma-4-e4b-judicialpredict-en` LoRA on RunPod.
* **S11.x — Retrain champion with real ideology features.**

---

## Risks

| Risk | Mitigation |
|---|---|
| MQ judge IDs ("justiceID" in the release) don't line up with our `judges.source_id` | Three-pass matcher (exact court+name, name-only, last-name+court) reused from S7 — same `--report` flag for human review of misses. |
| Operator sees "MARTIN-QUINN 1991" and assumes we have current data on a recently-confirmed justice | Disclosure copy: "Most recent term we have a public score for; not necessarily the current term." Tooltip on the chip names the methodology and the source URL. |
| MQ vs DIME divergence on a judge confuses operators (e.g. DIME +0.5, MQ -0.2) | Compliance footer explicitly names the precedence rule. Only one chip rendered — the one that actually drove the prefill. |
| Latest-score-only resolver hides a meaningful drift in late-career judges | Sprint 9 adds per-case-date lookup; S8's known limitation, documented in MODEL_CARD. |

---

## Definition of done

* `mqs-ingest` lands at least one synthetic SCOTUS justice into
  `bio.mqs` on the local dev DB.
* `extractFeatures` returns `ideologySource=martin_quinn` for that
  justice, with `ideologyTerm` populated.
* Intake form shows the MARTIN-QUINN chip on the prefill path.
* Case detail compliance footer enumerates both DIME and MQ as
  potentially-used Tier-A sources, with the precedence rule.
* DIME path from Sprint 7 still works for a judge that has only DIME data.
