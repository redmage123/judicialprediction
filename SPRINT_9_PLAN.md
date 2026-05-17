# Sprint 9 — JudicialPredict

**Theme:** Third Tier-A ideology source lands. Judicial Common Space (JCS)
scores extend Martin-Quinn coverage beyond SCOTUS to federal circuit and
district judges via Epstein/Martin/Quinn/Segal joint-scaling. The intake
form and compliance disclosure carry all three sources with explicit
precedence. The matcher / provenance plumbing established in Sprints 7-8 is
now stable enough that this slice is mostly a CSV swap + new bio-key.

**Window:** 2026-06-30 → 2026-07-21 (3 weeks).

---

## Carry-forward from Sprint 8

None blocking. Sprint 8 shipped Martin-Quinn ingest + temporal `bio.mqs`
schema + MQ-vs-DIME precedence + per-source UI chips.

A handful of audit recommendations from 2026-05-17 landed alongside this
sprint outside the Sprint-9 ticket scope:

* P1 jurisdiction wire-format mapping fix at the gateway.
* UX-1 cookie banner overlap.
* UX-6 policy pages under app shell.
* GraphQL introspection prod guard.
* "trained on synthetic data" dashboard disclosure.

---

## Goals

1. **JCS ingest** lives — a Rust crate reads the public Epstein/Martin/Quinn
   joint-scaling release, normalises names, matches against the existing
   `judges` table, writes the per-judge JCS score series and a
   `latest_score` / `latest_term` snapshot to `judges.bio.jcs`.
2. **Coverage expands** — JCS covers federal Circuit + District judges as
   well as SCOTUS justices, so the prefill path now resolves ideology for
   judges who have no DIME / MQ rows.
3. **Three-source precedence** — `MQ > JCS > DIME`. MQ is voting-record at
   the SCOTUS level (most direct signal); JCS extends voting-record-derived
   signal to lower courts via joint-scaling; DIME is the campaign-finance
   fallback. UI surfaces whichever source actually fired.
4. **Plumbing stays cheap.** The third ingest crate should mostly be a CSV
   reader + new `bio.<key>` patch; the matcher and provenance helpers stay
   shared via the dime-ingest crate.

What we are **not** doing in Sprint 9:

* Per-case provenance persistence. The compliance footer still describes
  *available* Tier-A sources, not the source that fired at prediction
  time. Sprint 10 picks this up via a `cases.ideology_provenance` JSONB
  column.
* Retraining the champion on real ideology features. Three sources is
  enough breadth; still waiting on attorney data + corpus growth so
  MODEL_CARD's calibration plot can be honest.
* Attorney ideology. Still gated on the state-bar bulk ingest.

---

## Background — what JCS actually is

The Judicial Common Space (Epstein, Martin, Quinn, Segal, 2007) extends the
SCOTUS-only Martin-Quinn estimator down to federal Circuit and District
judges. The methodology joint-scales a judge's appointing-senator NOMINATE
scores (Common Space) onto the same dimension as Martin-Quinn's SCOTUS
ideal points, producing a comparable cross-court ideology measure.

Public release: [https://epstein.wustl.edu/judicial-common-space](https://epstein.wustl.edu/judicial-common-space).
CSV is per-judge with columns approximating
`(judge_name, court, jcs, scale, ...)`. We treat it like DIME — a single
static drop with a per-judge scalar — though the underlying methodology
exposes per-term variation that Sprint 10+ can wire up if useful.

Why this third (after DIME + MQ):

* Closes the coverage gap. DIME covers SCOTUS + state high courts; MQ
  covers SCOTUS only. Federal Tax Court + Circuit + District judges are
  most of our actual workload — JCS reaches them.
* Re-uses the per-judge scalar pattern proven by DIME — quick to land.
* Compatible with the bio.mqs `scores[] + latest_score` envelope so
  Sprint 10's per-case-date lookup work covers all three sources by
  changing one resolver.

---

## Tickets

### S9.1 — Plan doc (this file)

**Owner:** dev
**Acceptance:** `SPRINT_9_PLAN.md` committed.

### S9.2 — Schema doc-only migration: `bio.jcs`

**Owner:** dev-backend
**Acceptance:**

* Migration `2026MMDDHHMMSS_jcs_provenance.sql` updates the
  `COMMENT ON COLUMN judges.bio` to describe `bio.jcs`. Reuses the GIN
  index added in S7.2.
* Documented JSONB shape:
  ```json
  {
    "jcs": {
      "score":            -0.41,
      "scale":            "epstein-2018",
      "release":          "jcs-2018-v1",
      "source_id":        "<emqs-judge-id>",
      "ingested_at":      "2026-07-01T10:00:00Z",
      "match_confidence": "exact|name_only|last_name+court"
    }
  }
  ```

### S9.3 — Rust crate: `jcs-ingest`

**Owner:** dev-backend
**Acceptance:**

* `rust/jcs-ingest/` workspace member, binary `jcs-ingest`.
* `ingest --csv <path> --tenant-id <uuid> [--report] [--release] [--dry-run]`.
* Re-uses `dime_ingest::matcher::match_row` AND `dime_ingest::parser`
  preprocessing helpers — no duplication.
* CSV columns understood: `judge_name`, `court`, `jcs`, `judge_id`
  (or `nominate_id`). Extra columns ignored.
* Patches `judges.bio.jcs` with `score + scale + release + source_id +
  ingested_at + match_confidence`.

### S9.4 — Synthetic JCS fixture + tests

**Owner:** dev-backend
**Acceptance:**

* `rust/jcs-ingest/fixtures/jcs-mini.csv` — 25 rows covering Circuit +
  District + SCOTUS judges; includes a NULL jcs row and an unmatched
  row to exercise the report path.
* `cargo test -p jcs-ingest` — at least 4 cases (parser happy path, NULL
  jcs, name-match exact + fallback paths).

### S9.5 — Gateway resolver reads `bio.jcs`

**Owner:** dev-backend
**Acceptance:**

* `extract_features_from_text` SELECT gains
  `(bio->'jcs'->>'score')::float8 AS jcs_score`
  and `(bio->'jcs'->>'release') AS jcs_release`.
* Precedence in the match-block: `mqs > jcs > dime`. JCS uses the same
  `|score|/2.0` scaling as DIME (JCS values are on the same scale as
  Common Space NOMINATE, roughly [-1, 1] but stored in a [-2, 2]-style
  band for compatibility with DIME).
* `ideology_source` enum gains the `judicial_common_space` variant; UI
  recognises it.

### S9.6 — UI: JCS chip + compliance update

**Owner:** dev-frontend
**Acceptance:**

* Intake form: indigo **JCS** chip when JCS fired (distinct from
  emerald MQ + blue DIME).
* Case detail compliance footer: Tier-A sources list now enumerates
  DIME + MQ + JCS with the three-way precedence rule. Methodology
  citation linked.

### S9.7 — Smoke + commit + push

**Owner:** dev
**Acceptance:**

* `scripts/jcs-smoke.sh` — mirrors the dime/mqs scripts. Ingests the
  fixture, asserts at least one judge gets `bio.jcs`, probes
  extractFeatures, verifies `judicial_common_space` source.
* `cargo test --workspace` clean.
* Push to `origin/main` after rebase.

---

## Out of scope (Sprint 10+)

* **S10.x — Per-case provenance persistence** (`cases.ideology_provenance`
  JSONB col).
* **S10.x — Date-aware MQ / JCS resolver** (currently latest-snapshot).
* **S11.x — Attorney ideology (state-bar + FEC joint-scaling).**
* **S12.x — Retrain champion on real-corpus + real ideology features.**

---

## Risks

| Risk | Mitigation |
|---|---|
| JCS coverage gaps (sparse for state courts and very recent appointees) | Three-source precedence — DIME picks up state-court judges that JCS doesn't cover. Document in MODEL_CARD. |
| Three precedence levels confuse operators | Only one chip rendered per prediction (per S8 convention). Compliance footer states the rule plainly. |
| JCS uses a different ideology scale than DIME / MQ | Each source carries its own scale tag in `bio.<src>.release`; resolver clamps to [0, 1] before the model sees it; rationale documented in code. |

---

## Definition of done

* `jcs-ingest` writes `bio.jcs` for at least one synthetic judge on the
  local dev DB.
* Three-source precedence (`MQ > JCS > DIME`) verified end-to-end:
  - MARSHALL (MQ+DIME, maybe +JCS) → `martin_quinn`
  - A Tax-Court judge (DIME only) → `bonica_dime`
  - A Circuit-Court judge with only JCS data → `judicial_common_space`
* `cargo test --workspace` clean.
* UI chip + compliance footer reflect JCS provenance.
* All Sprint-7 / Sprint-8 smoke scripts continue to pass.
