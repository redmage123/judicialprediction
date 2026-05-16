# Sprint 7 — JudicialPredict

**Theme:** First real Tier-A feature lands. Bonica DIME (campaign-finance
based) judge-ideology scores flow from public source data into the
intake-form prefill and the compliance disclosure, end to end. Lays the
ingest / name-matching / provenance pattern that Sprints 8+ will reuse for
Martin-Quinn (judges, time-varying) and JCS (Judicial Common Space).

**Window:** 2026-05-17 → 2026-06-07 (3 weeks).

---

## Carry-forward from Sprint 6

None blocking. Sprint 6 landed:

* model v1 trained + champion (`aad0f488`),
* PDF upload + CSV bulk import,
* `/v1/cases` REST + PAT auth,
* OIDC SSO,
* GDPR / WCAG / PEN audit + fixes (commits `a26cfe5`, `3540f86`, `f4a6ebd`).

A handful of S6 candidates were carved off and never opened (S6.16 OCR,
S6.17 dedupe `create_case`, S6.18 OpenAPI/Swagger, S6.19 per-PAT rate
limit, S6.20 PAT admin UI). They remain in the backlog; **not** Sprint 7.

---

## Goals

1. **Bonica DIME ingest** lives — a Rust crate reads the public DIME judge
   CSV, normalises names, matches against the existing `judges` table, and
   writes the per-judge cfscore + provenance into `judges.bio.dime`.
2. **Prefill path uses DIME** — when an operator pastes a prior opinion and
   we already have a DIME cfscore for the writing judge, the extracted
   `ideologyDistance` suggestion comes from real data, not synthetic.
3. **Compliance disclosure** surfaces the source — the case detail page
   names every Tier-A source that fed the prediction (`Bonica DIME 2014
   release`, etc.), versioned, so the printout in a partner's hand can
   defend the recommendation.
4. **Pattern is reusable.** Sprint 8 (Martin-Quinn) and Sprint 9 (JCS) can
   land by copying the dime-ingest crate, swapping the CSV reader and the
   bio-key, and reusing the same name-matcher + provenance helpers.

What we are **not** doing in Sprint 7:

* Retraining the champion model on real ideology features. The model still
  consumes operator-typed `ideologyDistance`; the value is just pre-filled
  from DIME when available. Retraining waits until ≥3 ideology sources are
  live so the comparison plot in MODEL_CARD is meaningful.
* Attorney-side ideology. The `attorneys` table doesn't exist yet; that's
  Sprint 9+ and depends on a different bulk-data path (state bar rolls,
  FEC contribution name-matching).
* Court-of-Federal-Claims and bankruptcy judges. DIME's judge release is
  Article III + state high courts; lower-court coverage is sparse.
* Time-varying / dynamic cfscores (`cfscore_dyn` column). Sprint 8 picks
  these up as part of the Martin-Quinn track because the temporal model is
  the same.

---

## Background — what Bonica DIME actually is

Adam Bonica's *Database on Ideology, Money in Politics, and Elections*
(DIME) is a public Stanford-hosted release that maps individuals and
organisations to a one-dimensional ideal-point ("cfscore" or
*campaign-finance score*) by joint-scaling their FEC and state-level
contribution patterns. The judge release links the scaled score to
federal Article III judges and state high-court justices via Senate
confirmation hearing records and state-court appointments. Values run
roughly from −2 (most liberal) to +2 (most conservative), centred near 0.

**Public source:**
[https://data.stanford.edu/dime](https://data.stanford.edu/dime). Released
under their academic terms; we treat the file like any other bulk source
(local copy, hashed for reproducibility, version-stamped in the DB).

Why this slice first (vs. Martin-Quinn):

* CSV-shaped, no NLP needed.
* Validated against decades of academic work — defensible in a deposition.
* Per-judge static value — clean schema, no temporal joins.
* The name-matching infrastructure we build here ports straight to
  Martin-Quinn and JCS.

---

## Tickets

### S7.1 — Plan doc (this file)

**Owner:** AI-Elevate / dev
**Acceptance:** `SPRINT_7_PLAN.md` committed.

### S7.2 — Schema: `judges.bio.dime` JSONB block

**Owner:** dev-backend
**Acceptance:**

* New SQL migration `2026MMDDHHMMSS_dime_provenance.sql` lands in
  `rust/feature-store/migrations/`. It is a no-op against existing data
  (provenance lives inside the `bio` JSONB column which already exists);
  the migration adds a `COMMENT ON COLUMN` describing the `bio.dime` shape
  and creates a partial GIN index for fast "judges that have DIME data"
  queries.
* Documented JSONB shape:
  ```json
  {
    "dime": {
      "cfscore": -0.41,
      "release": "dime-2014-judges-v1.0",
      "source_id": "<bonica-judge-id>",
      "ingested_at": "2026-05-17T10:00:00Z",
      "match_confidence": "exact|court+name|fuzzy"
    }
  }
  ```

### S7.3 — Rust crate: `dime-ingest`

**Owner:** dev-backend
**Acceptance:**

* `rust/dime-ingest/` workspace member, binary `dime-ingest`.
* Subcommand `ingest --csv <path> --tenant-id <uuid>`.
* Pure CSV parsing (the `csv` crate) — no live HTTP. Operator drops the
  Bonica file at `data/dime/judges.csv` (gitignored), or passes `--csv`.
* Name normaliser: lowercase, strip middle initials, strip honorifics,
  collapse whitespace. Identical helper used by `extract_features_from_text`
  via the existing `ingest_fetcher::normalize_judge_name`. (Re-export, do
  not fork.)
* Match algorithm (defence-in-depth):
  1. Exact `(normalized_name, primary_court_id)` match. Confidence `exact`.
  2. Exact `normalized_name` match where the judge has no `primary_court_id`
     yet. Confidence `name_only`.
  3. Skip otherwise — log to `--report <path>` for human review.
* Idempotent: re-running overwrites `bio.dime` with the same content.
  Counter `judges_updated / rows_unmatched / rows_skipped_no_cfscore`
  reported at exit.

### S7.4 — Fixture + parser unit tests

**Owner:** dev-backend
**Acceptance:**

* `rust/dime-ingest/fixtures/dime-judges-mini.csv` — 25 rows in the real
  DIME format. Mix of: exact court+name matches, name-only matches,
  matches we deliberately can't make (different court), and one row with
  a NULL cfscore that the importer must skip.
* `cargo test -p dime-ingest` covers:
  * CSV header detection (DIME has stable column order but we use header names).
  * Name normaliser: `Hon. John D. Smith, Jr.` → `john smith jr`,
    `O'Connor, Sandra Day` → `sandra day o'connor`.
  * NULL / empty cfscore skipped, not zeroed.
  * Two judges with the same normalised name on different courts
    disambiguated by court_id.

### S7.5 — Feature pipeline reads DIME

**Owner:** dev-backend
**Acceptance:**

* `extract_features_from_text` (the `extractFeatures` query + `createCase`
  prefill path) gains an `ideology_distance` field on `ExtractedFeatures`
  computed as `(cfscore - reference_anchor).abs()` where the reference
  anchor is the current operator's tenant default (configurable, default
  0.0 / "centre"). Null when DIME doesn't have the judge.
* Returned `extracted_from` enum gains a `BonicaDime` variant carrying the
  release tag so the UI can render provenance.
* Existing operator-typed flow unchanged — typing into the form still wins.

### S7.6 — UI: DIME badge + compliance disclosure

**Owner:** dev-frontend
**Acceptance:**

* Intake form (`/case/new`): when prefill returns a DIME source, the
  `Ideology distance` field shows a small **Bonica DIME 2014** badge with
  a tooltip linking to `/privacy#sources` and showing the raw cfscore.
* `/case/[id]` results-view footer gains a "Tier-A sources used" line that
  enumerates the per-feature provenance for that prediction
  (judge-severity from CourtListener, ideology from Bonica DIME, etc.).
  When a feature was operator-typed without a source, the line says
  "operator-supplied".

### S7.7 — End-to-end smoke

**Owner:** dev / QA
**Acceptance:**

* `scripts/dime-smoke.sh` ingests the fixture into the dev DB, calls
  `extractFeatures` with a fixture opinion text whose judge is in the
  fixture, asserts the returned `ideology_distance` is non-null with a
  `BonicaDime` source tag.
* Manual UI walk-through: paste fixture opinion → see Bonica DIME badge →
  Run prediction → confirm Tier-A sources line in results-view names DIME.

### S7.8 — Commit + push

**Owner:** dev
**Acceptance:**

* One coherent commit per ticket (S7.2–S7.7). Plan doc commits first.
* `pnpm typecheck` (web), `cargo check -p dime-ingest -p api-gateway`,
  and the new dime-smoke script all green before push.

---

## Out of scope (Sprint 8+ carve-offs)

* **S8.x — Martin-Quinn (judges, time-varying).** Reuses the dime-ingest
  pattern but stores `bio.mqs.scores[]` keyed by term.
* **S8.x — JCS (Judicial Common Space).** Reuses dime-ingest pattern but
  scales Martin-Quinn against Common Space for cross-judge comparability.
* **S9.x — Attorney ideology proxy.** Needs an `attorneys` table first,
  plus state-bar roll ingest + FEC name-matching.
* **S10.x — HEXACO / MFT.** Needs the gemma-4-e4b judicial LoRA from the
  RunPod fine-tune track; the EUR-LEX LoRA is the wrong corpus.
* **S11.x — Retrain champion with real ideology features.** Hold until at
  least DIME + Martin-Quinn + one attorney feature are live, so the model
  card's calibration plot has something honest to say.

---

## Risks

| Risk | Mitigation |
|---|---|
| DIME judge names don't line up cleanly with CourtListener-derived judges (initials, suffixes, name changes after marriage) | Multi-pass matcher + `--report` flag listing unmatched rows for human review. Document the manual disambiguation path. |
| Operator confusion when ideology comes from cfscore (campaign-money proxy, not voting record) | Tooltip on the DIME badge spells out the methodology in one sentence; compliance footer in results-view names the source explicitly. |
| Bonica file licence terms drift | Treat the local copy as a versioned artefact; `release` tag in `bio.dime` ties each prediction to a specific release. |
| Operators interpret a `cfscore` of -1.5 as "this judge will rule for the plaintiff" | Disclosure copy: "campaign-finance ideology proxy; not a vote-direction prediction". UI never shows the raw cfscore without context. |

---

## Definition of done

* Bonica DIME judge data, loaded against the local dev DB from the
  synthetic fixture, flows through the prediction loop end-to-end.
* The intake form and the results view both show the DIME provenance.
* The next sprint can land Martin-Quinn by copying the dime-ingest crate
  and changing one CSV reader. (Pattern proven.)
