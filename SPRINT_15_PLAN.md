# Sprint 15 — JudicialPredict

**Theme:** Get off the synthetic corpus. Sprint 14 proved the
extract-features pipeline is healthy (5/109 → 52/109 labelled after the
cafc + plural-form patches) but the n=41 retrain lost decisively to the
n=2000 synthetic champion. The only honest path out is **a real corpus
of meaningful size with real labels for the bulk of it**. This sprint
ingests four new sources, extends outcome detection to handle them,
loads pre-coded labels where they exist, and retrains the full four-model
ensemble (XGBoost + LightGBM + CatBoost + LR + stacked blender) on the
combined real corpus.

**Window:** 2026-05-19 → 2026-06-09 (3 weeks).

---

## Carry-forward from Sprint 14

* Sprint 14 introduced `detect_outcome_for_court`; the dispatch table only
  knows `tax` and `cafc`. Sprint 15 needs to extend it to the rest of the
  federal-court family (district, circuit, scotus, bankruptcy).
* Sprint 14 surfaced the cafc judge-match gap (most cafc opinions had
  `judge_severity = NULL` in the export because no panel member appears
  in our small KG). FJC Biographical Directory ingest fixes this directly.
* `python/ml-inference-svc/data/real_corpus_v2.parquet` (41 rows) is the
  baseline number to beat. The Sprint 12.5 LR (Brier 0.1662, n=2000
  synthetic) is the champion to dethrone.

---

## Source survey (what we're ingesting and why)

| Source | Shape | What it adds | Effort |
|---|---|---|---|
| **Caselaw Access Project (CAP)** static.case.law | JSONL bulk dumps | ~6.7M US opinions across all jurisdictions; the volume play | High |
| **Supreme Court Database (SCDB)** scdb.wustl.edu | CSV, ~30K rows | Hand-coded outcome / direction / issue area for every SCOTUS case since 1791 — **the only pre-labelled source** | Low |
| **FJC Biographical Directory** uscourts.gov | CSV, ~3500 judges | Federal-judge metadata + tenure + appointing president (fixes cafc judge-match gap, feeds severity rollup) | Low |
| **govinfo USCOURTS** govinfo.gov API | XML | Clean federal appellate / district / bankruptcy opinions; complements CL coverage gaps | Medium |
| **Tax Court DAWSON** ustaxcourt.gov | HTML + slip PDFs | Direct from the court; cleaner than scraping CL's tax slice | Low |

**Recommended deferrals to Sprint 16:**
* **PACER/RECAP** — RECAP archives PACER **dockets** (procedural history),
  not opinions. Useful for a better procedural-motion-count feature but
  doesn't give us new labelled cases; reroute to Sprint 16 when we
  retire the regex-based motion counter.
* **PTAB / USPTO patent appeals** — different domain (patent
  invalidity, not civil/criminal triage). Expanding into IP litigation
  is a product decision, not a data decision; punt until the operator
  signals demand.

---

## Goals

1. **Wire SCDB and FJC into the schema and KG.** SCDB gives us free
   binary labels for SCOTUS; FJC closes the judge-match gap for the
   federal corpus.
2. **Bulk-ingest CAP + govinfo + DAWSON** at a realistic scale
   (~50,000–100,000 federal-court opinions). Full 6.7M is a Sprint
   16+ ask — we want the slice that maps onto the existing
   `{Federal, California, New_Jersey}` jurisdiction allowlist plus
   SCOTUS.
3. **Extend outcome detection to the rest of the federal family.**
   Sprint 14 handled `tax` + `cafc`; this sprint adds `scotus`,
   federal district, federal circuit (non-Federal-Circuit), and
   bankruptcy. SCDB-labelled SCOTUS doubles as the validation set.
4. **Retrain the full four-model ensemble** on the combined real
   corpus. Restore XGBoost + LightGBM + CatBoost + LR + the K=5
   stacked blender path that Sprint 12.5 added — the n=41 Sprint 14
   experiment used only LR because GBMs need volume; now we have it.
5. **Update MODEL_CARD honestly.** New row count, new metric table, new
   limitations section (state courts still unlabelled, attorney /
   ideology / materiality features still neutral-prior).

What we are **not** doing in Sprint 15:

* State-court bulk ingest at the CAP scale. Federal-only for the first
  retrain; state-court outcome conventions vary enough per state that
  we'd need 50 dispatch entries and that's its own sprint.
* Real attorney_win_rate / ideology_distance / materiality_score
  features. Those still come from the gateway resolver (DIME/MQ/JCS for
  ideology) or default to NEUTRAL_FILL — feature engineering is a
  separate Sprint 17 candidate.
* A learned outcome classifier (use SCDB-labelled cases as training
  data to predict outcomes on CAP). Tempting and probably the right
  Sprint 16 move; not in scope here.
* Switching MLflow's tracking backend from file-store to sqlite/postgres
  (already a FutureWarning; deferred so retrain doesn't double up with
  infra churn).

---

## Tickets

### S15.1 — Plan doc (this file)

### S15.2 — Database schema extensions

* New table `case_outcome_labels` (run_id, opinion_id, source, outcome,
  confidence, ingested_at). One row per pre-coded label so SCDB and
  future labelled sources don't collide.
* New columns on `judges`: `appointing_president`, `appointment_date`,
  `senior_status_date`, `confirmed_by_senate` (nullable). Migrated via
  Diesel + a forward migration; rollback drops the columns.
* `case_documents.bulk_source` enum: `courtlistener`, `cap`, `govinfo`,
  `dawson`. Default `courtlistener` for existing rows. Lets us slice
  metrics by source.

### S15.3 — SCDB ingest (smallest, do first)

* New CLI `scripts/scdb-ingest.sh` pulls the SCDB modern.csv +
  legacy.csv from washingtonu.edu, joins by docket-number to existing
  SCOTUS rows in `case_documents`, and inserts `case_outcome_labels`
  rows with `source = 'scdb'`.
* Mapping: SCDB `partyWinning = 1` → `petitioner`,
  `partyWinning = 0` → `respondent`. SCDB-coded "ambiguous" rows
  drop (no usable label).
* Idempotent: re-runs by `(source, opinion_id)` unique constraint.

### S15.4 — FJC Biographical Directory ingest

* New CLI `scripts/fjc-ingest.sh` pulls FJC's "judges.csv" (~3500
  rows), upserts each federal judge into `judges` with
  `normalized_name`, `appointing_president`, `appointment_date`,
  `confirmed_by_senate`. Existing rows (DIME/MQ/JCS-linked) keep
  their bio data; new fields merge into `bio` JSONB.
* Smoke: re-run extract-features on the cafc slice and confirm the
  judge_match rate goes from ~14% (2/14 cafc opinions had a match
  pre-FJC) up past 70%.

### S15.5 — CAP bulk ingest (federal slice)

* New module `rust/ingest-fetcher/src/cap.rs`. Downloads CAP's
  per-jurisdiction JSONL dumps for: U.S. Supreme Court, U.S. Courts of
  Appeals, U.S. District Courts. Reuses the existing tarball/streaming
  pattern from `parse_tarball.rs`.
* Schema mapping: CAP's `case.name`, `case.opinions[].text`,
  `case.decision_date`, `case.court.id` → existing `case_documents`
  schema with `bulk_source = 'cap'`.
* Gate: ingest cap at ~50,000 federal opinions for Sprint 15. Wider
  pulls are a Sprint 16 ask once we know how the ensemble responds.
* Disk: ~5–10 GB compressed JSONL on the dev box; full text stays in
  postgres `text` column (not `tsvector`-indexed yet — that's Sprint 17).

### S15.6 — govinfo USCOURTS ingest (federal complement)

* New module `rust/ingest-fetcher/src/govinfo.rs`. Hits the
  govinfo.gov USCOURTS bulkdata API for any post-2010 federal
  opinions not already in CAP (CAP coverage thins for the most recent
  ~3 years).
* Lower priority than S15.5; if CAP coverage proves sufficient,
  drop govinfo from this sprint and revisit in S16.

### S15.7 — Tax Court DAWSON augmentation

* `scripts/dawson-ingest.sh` scrapes the DAWSON public opinion feed
  for tax-court opinions not already in CourtListener's `tax` slice.
  Marks `bulk_source = 'dawson'`.
* Same outcome-detection path as the existing tax slice — no new
  patterns needed (DAWSON publishes the same "Decision will be entered"
  language as CL's tax opinions).

### S15.8 — Outcome detection: federal-court coverage

* `detect_outcome_for_court` gains dispatch entries:
  * `scotus` → new appellate scanner tuned for SCOTUS syllabus
    language ("The judgment of the Court of Appeals is affirmed /
    reversed / vacated"). Validated against SCDB labels.
  * `ca[0-9]+` (CA1-CA11, CADC) → same appellate scanner as `cafc`
    (the disposition block conventions are identical).
  * federal-district (`nyd`, `cad`, `txnd`, …) → new scanner for
    district-court Rule 12 / Rule 56 / Rule 41 dispositions
    ("DENIED", "GRANTED IN PART", "DISMISSED").
  * `bankr` → defer to S16 (different convention, low volume).
* PASS criterion: outcome-label coverage ≥ 30% across the new federal
  slice (better than Sprint 14's 38% on cafc, much better than tax's
  12%).

### S15.9 — SCDB-validated detector calibration

* Use SCDB-labelled SCOTUS opinions as a held-out validation set.
  Run `detect_outcome_for_court("scotus", text)` on each and report
  precision / recall vs the SCDB ground truth.
* Gate: detector precision ≥ 0.90 on SCOTUS (we tolerate lower
  recall — we'd rather skip an ambiguous opinion than mislabel one).
  If precision is below 0.90, fall back to using SCDB-labelled
  opinions only and skip detector-derived SCOTUS labels.

### S15.10 — Full ensemble retrain

* Build `data/real_corpus_v3.parquet` from `case_documents` joined to
  `case_outcome_labels` (preferring SCDB-coded over detector-derived
  when both exist).
* Run `python scripts/train_first_models.py --data
  data/real_corpus_v3.parquet` — the existing trainer (not the
  small-N `train_real_v1.py`). This trains XGBoost + LightGBM +
  CatBoost + LR + the K=5 stacked blender.
* Hyperparameters: keep the Sprint 12.5 defaults for a first cut.
  Hyperparameter search is a Sprint 17 candidate, not this sprint.
* Champion picked by Brier on a held-out 20% slice **stratified by
  bulk_source** so we don't get a champion that wins only on the CAP
  bulk and tanks on the smaller CL slice.

### S15.11 — Promotion gate

* Real-corpus v3 champion promotes **only if**:
  1. Brier ≤ 0.18 (the Sprint 12.5 synthetic champion is 0.1662 —
     we'll accept a modest regression on Brier in exchange for real
     data; >0.18 means the model is worse than coin flip on this
     corpus and should not ship).
  2. ECE ≤ 0.08 (Sprint 12.5 is 0.0471; we accept some calibration
     loss because real outcomes are non-logistic).
  3. Variance sweep ≥ 0.10 pWin spread on the canonical 20-input
     probe (the same gate Sprint 12 used).
  4. Source-stratified Brier doesn't differ by more than 50% between
     bulk_source slices (sanity check against a CAP-only winner).
* If any gate fails: keep Sprint 12.5 LR as champion, document the
  gap in MODEL_CARD, and feed the failure analysis into Sprint 16.

### S15.12 — MODEL_CARD update

* New "Sprint 15 — real-corpus v3" section with:
  * Per-source row counts (CL, CAP, govinfo, DAWSON).
  * SCDB-labelled vs detector-labelled breakdown.
  * Per-model and ensemble metrics.
  * Stacked-blender meta-LR coefficients (which base model the
    blender trusts most on real data).
  * Updated "Known limitations" — state courts still unlabelled,
    attorney / ideology / materiality features still NEUTRAL_FILL.

### S15.13 — Smoke + commit + push

* All existing smokes still PASS (DIME / MQ / JCS / provenance /
  date-filed / model-variance-sweep).
* New smoke `scripts/scdb-detector-smoke.sh` runs S15.9's validation
  and asserts precision ≥ 0.90.
* Commit per sub-sprint (S15.2 through S15.12 are independent commits
  on `main` — keeps the diff reviewable).

---

## Out of scope (Sprint 16 candidates)

* **State-court bulk ingest.** Each state has its own disposition
  vocabulary; needs per-state detector entries or (better) a learned
  classifier trained on SCDB + S15's detector outputs.
* **PACER/RECAP docket integration.** Procedural history → better
  `procedural_motion_count` feature. Real value but not on the
  outcome-label path.
* **PTAB / USPTO patent appeals.** Different product surface.
* **Learned outcome classifier.** Train a small transformer or simple
  LR-on-tfidf classifier using SCDB labels as ground truth; apply
  back to the unlabelled CAP corpus. The honest scaling answer.
* **Switching MLflow to a sqlite/postgres tracking backend.**
* **`tsvector`-indexing the bulk corpus.** Sprint 17 search candidate.
* **Real attorney / ideology / materiality features** from real data
  (currently all NEUTRAL_FILL). Each is its own sprint:
  * attorney_win_rate ← attorney name extraction + per-attorney
    tally rollup.
  * ideology_distance ← already wired (DIME/MQ/JCS); just needs the
    judge KG to be populated, which FJC ingest does.
  * materiality_score ← needs a definition before it needs an
    implementation.

---

## Risks

| Risk | Mitigation |
|---|---|
| CAP JSONL volume blows up disk on the dev box | Cap S15.5 at ~50K opinions; full corpus is Sprint 16. Postgres on the dev box has ~80 GB free; 50K opinions × ~50 KB = ~2.5 GB. |
| Outcome detector precision is low on districts and circuits | S15.9's SCDB-validation gate falls back to "SCDB-only labels for SCOTUS, detector-derived for the others". If the detector is unsalvageable, defer non-SCOTUS to a learned classifier in S16. |
| Real corpus has features mostly NEUTRAL_FILL → champion learns nothing useful | Document honestly in MODEL_CARD; the promotion gate (Brier ≤ 0.18) will catch this and refuse to promote. We get a published "we tried, it didn't beat synthetic" outcome rather than shipping a bad model. |
| FJC normalised-name mismatch with the existing DIME/MQ judges | Use the same `normalize_judge_name` helper that S5.6 added; on conflict, prefer the existing row's DIME/MQ scores and merge FJC's biographic fields into `bio`. Add a smoke that asserts no DIME/MQ scores were overwritten. |
| GBM training on 50K rows is slow on CPU | The Sprint 13 GPU enablement (xgboost device=cuda, lightgbm device=gpu, catboost task_type=GPU) covers this. End-to-end ensemble training should stay under ~15 minutes per run. |
| MLflow file-store struggles past ~50 runs in one experiment | Acknowledged FutureWarning; defer the postgres backend migration to Sprint 16. |

---

## Definition of done

* `case_outcome_labels` and `judges.bio` extensions migrated.
* SCDB ingest run; SCOTUS labels in `case_outcome_labels`.
* FJC ingest run; cafc judge-match rate ≥ 70% on re-run extraction.
* CAP federal-slice ingest at ≥ 30,000 opinions in `case_documents`
  with `bulk_source = 'cap'`.
* govinfo + DAWSON ingest run (or explicitly deferred with a
  one-line note in MODEL_CARD).
* `detect_outcome_for_court` dispatches for all federal court types
  with SCDB-validated precision ≥ 0.90 on SCOTUS.
* `data/real_corpus_v3.parquet` exists with ≥ 15,000 labelled rows.
* Full four-model ensemble + stacked blender trained on v3 with
  metrics logged to MLflow.
* `mlruns/champion.json` either points at a v3 winner (if all
  promotion gates pass) or remains pinned to Sprint 12.5 with an
  explicit "did not promote" note in MODEL_CARD.
* All prior smokes still PASS; new SCDB-detector smoke PASSES.
* Sprint 15 plan + sub-sprint commits all on `origin/main`.
