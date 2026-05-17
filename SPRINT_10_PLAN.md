# Sprint 10 — JudicialPredict

**Theme:** Per-case ideology provenance lands. The compliance footer on
`/case/[id]` stops describing what we *could* have used and starts naming
exactly what we *did* use, frozen at prediction time alongside the model
version and conformal interval. Resolver also gains a date-aware MQ
lookup so cases tied to a historical date can pull the appropriate term's
score rather than the latest snapshot.

**Window:** 2026-07-22 → 2026-08-12 (3 weeks).

---

## Carry-forward from Sprint 9

None blocking. Sprint 9 shipped:

* JCS (Judicial Common Space) — third Tier-A source,
* three-source precedence MQ > JCS > DIME,
* the P1 jurisdiction wire-format fix,
* UX rollups for the cookie banner overlap + policy-page shell,
* the "Beta model" dashboard disclosure,
* prod-mode GraphQL introspection guard.

---

## Goals

1. **Per-case provenance persists.** When the gateway resolves an ideology
   score for a `createCase` call, it snapshots `(source, release, term,
   raw_score)` and writes the snapshot to `cases.ideology_provenance`
   alongside the prediction and recommendation. Subsequent reads of the
   case render the snapshot — never a current-state lookup that may have
   drifted.
2. **Case detail tells the truth.** The Tier-A sources disclosure on
   `/case/[id]` reflects the actual source used for that case, not the
   menu of sources we have available. When a case ran during the DIME-only
   era and JCS arrived later, the printed memo still names DIME.
3. **Date-aware MQ resolver.** `extract_features_from_text` accepts an
   optional `asOfDate` parameter. When supplied, the MQ branch picks the
   highest term with a non-null score that is ≤ the year of `asOfDate`;
   when omitted it falls back to today's `latest_term`. JCS and DIME stay
   single-point — their releases are static, not term-keyed.
4. **No regressions** on DIME / MQ / JCS smokes.

What we are **not** doing in Sprint 10:

* Adding a `cases.date_filed` column or exposing an as-of-date picker in
  the intake form. The gateway path is wired so a future sprint can add
  the column + UI in one ticket; today the value defaults to "today".
* Re-using `ideology_provenance` in the model itself (the model still
  consumes a single scalar `ideologyDistance`).
* Backfilling provenance for pre-Sprint-10 cases. The column is nullable
  and the UI renders the legacy "available sources" footer when it's
  null.

---

## Tickets

### S10.1 — Plan doc (this file)

### S10.2 — Migration: `cases.ideology_provenance` JSONB

* Migration `2026MMDDHHMMSS_cases_ideology_provenance.sql` adds the column
  nullable, no default. Pre-existing cases stay `NULL`.
* Documented shape:
  ```json
  {
    "source":      "martin_quinn",            // bonica_dime | martin_quinn | judicial_common_space
    "release":     "mqs-2023-v1",
    "raw_score":   -1.43,                     // source's native scale
    "term":        1972,                      // null for DIME / JCS
    "as_of_date":  "2026-07-22",              // when omitted: today
    "resolved_at": "2026-07-22T10:00:00Z"
  }
  ```

### S10.3 — Gateway: date-aware MQ resolver + provenance return

* `extract_features_from_text(pool, tenant_id, text, as_of_year)` —
  signature gains `as_of_year: Option<i32>`.
* When the MQ branch fires and `as_of_year` is supplied, walk `bio.mqs.scores[]`
  for the highest `term` ≤ `as_of_year` with non-null `post_mean`. Fall back
  to `latest_score` / `latest_term` when no eligible term exists.
* `ExtractedFeatures` gains an `ideologyTermResolution` field reporting how
  the term was chosen: `"latest"` | `"as_of_year"` | `"fallback_latest"`.

### S10.4 — `createCase` persists provenance

* `createCase` builds the provenance JSON from the resolved features and
  passes it to the `cases` insert. NULL when no source fired (operator
  typed every field manually).

### S10.5 — Case detail renders actual provenance

* `app/case/[id]/page.tsx` GraphQL query gains `ideologyProvenance`.
* `results-view.tsx` Tier-A sources footer: when `ideologyProvenance` is
  non-null, render the one source that fired with its release tag and
  term; when null, fall back to the existing "available sources" copy.

### S10.6 — Smoke + commit + push

* `scripts/provenance-smoke.sh`: create a case via the API, fetch it
  back, assert `ideologyProvenance.source` matches what `extractFeatures`
  returned for the same input.
* Re-run the three sprint smokes (DIME / MQ / JCS) — must stay green.

---

## Out of scope (deferred)

* **S11.x — `cases.date_filed` column + intake-form date picker.**
* **S11.x — Per-case-date lookup for JCS** (currently single-point; needs
  joint-scaling that varies by term).
* **S12.x — Retrain champion on real corpus + real ideology features.**
* **S12.x — Backfill provenance for legacy cases** (best-effort guess via
  current sources + created_at as as-of date).

---

## Risks

| Risk | Mitigation |
|---|---|
| Schema migration locks the `cases` table briefly | Column add is metadata-only on Postgres; no rewrite. Run during low traffic anyway. |
| Footer becomes inconsistent (some cases show actual provenance, others fall back) | Document the boundary in the footer ("Predicted before Sprint-10 — source not snapshotted"). UI handles both branches. |
| `as_of_year` regression on existing callers | Make it `Option<i32>` so callers that don't supply it get current behaviour byte-for-byte. |

---

## Definition of done

* `cases.ideology_provenance` populated for every new case where an
  ideology source fired.
* `/case/[id]` footer shows the actual source on Sprint-10+ cases,
  legacy footer on older cases.
* `as_of_year=1990` on MARSHALL (who has terms 1967-1972 in the fixture)
  returns the 1972 row; `as_of_year=1968` returns the 1968 row.
* All three sprint smokes (DIME / MQ / JCS) still green.
* `provenance-smoke.sh` green.
