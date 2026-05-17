# Sprint 11 — JudicialPredict

**Theme:** `cases.date_filed` lands. The intake form gains a date picker, the
gateway persists the operator-supplied filing date, and Sprint 10's
date-aware MQ resolver gets fed automatically — so a 1969 case pulls the
1969 MQ score, not today's snapshot. The dashboard's "DATE FILED" column
finally shows something real.

**Window:** 2026-08-13 → 2026-09-03 (3 weeks).

---

## Carry-forward from Sprint 10

None blocking. Sprint 10 shipped:

* `cases.ideology_provenance` nullable JSONB column,
* date-aware MQ resolver (`extract_features_from_text(as_of_year: Option<i32>)`),
* `extractFeatures(asOfYear: Int)` GraphQL arg,
* `createCase` persists provenance, `case(id)` returns it,
* results-view footer renders actual provenance with legacy fallback.

The piece that didn't ship in S10 — wiring `asOfYear` from a real case
date — was deferred to S11 because it needed a `cases.date_filed` column
plus a UI picker. That's exactly this sprint.

---

## Goals

1. **Schema for filing date.** New nullable `cases.date_filed` DATE
   column. Pre-Sprint-11 cases stay NULL; the dashboard falls back to
   `created_at` for display.
2. **Operator can set the filing date.** Intake form at `/case/new`
   gains a date picker. Default = today, but operators can move it back
   to the actual filing date.
3. **Resolver feeds itself.** When the operator submits a case with a
   `dateFiled` < today, the gateway passes
   `year(dateFiled)` to `extract_features_from_text(as_of_year=...)` so
   the MQ branch picks the term that was current when the case was
   filed.
4. **Dashboard shows the truth.** The "DATE FILED" column displays
   `date_filed` when populated, falls back to `created_at` when NULL.

What we are **not** doing in Sprint 11:

* Date-aware JCS or DIME. Both releases are single-point in our
  storage; per-term JCS needs a methodology decision (covered in the
  Sprint 9 plan as deferred work).
* Backfilling `date_filed` for legacy cases. Operators can edit the
  column manually if they want; we don't infer.
* Editing `date_filed` after createCase. Sprint 12 candidate.

---

## Tickets

### S11.1 — Plan doc (this file)

### S11.2 — Migration: `cases.date_filed` DATE column

* Migration `2026MMDDHHMMSS_cases_date_filed.sql` adds a nullable DATE
  column + a `(tenant_id, date_filed DESC NULLS LAST)` partial index so
  the dashboard's "recent by filing date" sort is cheap.

### S11.3 — Gateway: `createCase(dateFiled: NaiveDate)` + asOfYear wiring

* `Mutation::create_case` accepts an optional `dateFiled` arg.
* Gateway derives `as_of_year = dateFiled.year()` and passes it through
  to `extract_features_from_text` when the opinion-text prefill path
  fires. Resolver-only callers (the standalone `extractFeatures` query)
  keep accepting `asOfYear` directly.
* INSERT into `cases` writes `date_filed` (NULL when the arg was
  omitted).

### S11.4 — Intake form date picker

* `/case/new` adds an `<input type="date">` labelled "Filing date
  (optional)" defaulting to today. Helper text: "Used to pick the MQ
  term that was current when the case was filed."
* Form state gains `dateFiled: string` and `createCase` mutation
  variables forward it.

### S11.5 — Dashboard renders real `date_filed`

* `CasesDashboard` GraphQL query gains `dateFiled` on each row.
* `cases-table.tsx` displays `dateFiled` when present, else
  `createdAt`. Format MM dd YYYY in both branches.
* Sort changes to `ORDER BY COALESCE(date_filed, created_at) DESC`.

### S11.6 — Smoke + commit + push

* `scripts/date-filed-smoke.sh`:
  - createCase with `dateFiled=1969-06-15` for MARSHALL,
  - assert returned `ideologyProvenance.term == 1969` (date-aware MQ
    fired automatically),
  - assert `case(id)` returns `dateFiled == "1969-06-15"`,
  - assert listCases shows it.
* All four prior smokes (DIME / MQ / JCS / provenance) stay green.

---

## Out of scope (Sprint 12+)

* **S12.x — Editing `date_filed` after createCase.**
* **S12.x — Per-term JCS lookup** (needs methodology decision).
* **S13.x — Retrain champion on real corpus + real ideology features.**
* **S13.x — Backfill `date_filed` heuristically** from opinion-text
  parsing (the layer-3 extractor catches some, just not surfaced).

---

## Risks

| Risk | Mitigation |
|---|---|
| Operators leave the date as today even for historical cases, defeating the point | Helper text on the picker explains why it matters; results-view shows the term that fired so they can spot a mismatch. |
| Date picker UX is dev-only-ugly | Use native `<input type="date">` for now — same shape as the rest of the form, no extra deps. Sprint 12 can swap for a shadcn DatePicker. |
| Existing tests break on the new mutation arg | Make `dateFiled` nullable + optional everywhere. Existing callers pass nothing. |

---

## Definition of done

* `cases.date_filed` populated for every Sprint-11+ case created via the
  form.
* createCase with `dateFiled=1969-06-15` for MARSHALL writes `term=1969`
  into `ideology_provenance` (S10 wiring fed automatically).
* Dashboard displays the operator-supplied date.
* All five smokes (DIME / MQ / JCS / provenance / date-filed) PASS.
