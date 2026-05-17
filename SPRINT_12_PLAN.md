# Sprint 12 — JudicialPredict

**Theme:** Close the "Beta model" loop. The audit's amber banner has been
on every dashboard since Sprint 9, calling out that the champion was
trained on `synthetic_cases_v0.parquet` and collapsing every prediction
to ~50% / Settle. Sprint 11 fixed the silent jurisdiction encoding —
predictions should now vary across inputs. This sprint validates that,
regenerates the synthetic corpus with realistic variance, retrains the
ensemble, and softens (or replaces) the banner with a versioned, honest
disclosure.

**Window:** 2026-09-04 → 2026-09-25 (3 weeks).

---

## Carry-forward from Sprint 11

None blocking. Sprint 11 shipped `cases.date_filed` + intake date picker
+ auto-fed `asOfYear`. The MQ resolver now snaps to the correct historical
term based on the operator-supplied filing date.

---

## Goals

1. **Validate that the jurisdiction fix unstuck predictions.** Run a
   sweep through the live (v0) model with diverse inputs — civil vs
   criminal, Federal vs CA vs NJ, low vs high judge severity. If pWin
   now varies meaningfully (>=10 pp spread across the sweep), the v0
   model is actually usable as a baseline and the retrain is about
   improving fidelity, not unstucking dead features.
2. **Regenerate the synthetic dataset.** `synthetic_cases_v1.parquet`
   widens the feature distributions, adds realistic outcome correlations
   (judge_severity ↔ respondent wins, attorney_win_rate ↔ petitioner
   wins, ideology_distance has a mild non-monotonic effect), and
   maintains the same column schema so the training script stays
   unchanged.
3. **Retrain the ensemble.** XGBoost / LightGBM / CatBoost on v1.
   Champion picked by Brier. Logged to MLflow + champion.json — the same
   path the gateway already loads.
4. **Update MODEL_CARD honestly.** Sample size, Brier, ECE, log-loss,
   calibration plot, intended use, known limitations.
5. **Replace the Beta banner.** The "trained on synthetic_cases_v0" copy
   is wrong (we're on v1 now) and the audit recommendation called for a
   less alarmist version. Replace with a model-version stamp that links
   to the MODEL_CARD instead of an amber warning.

What we are **not** doing in Sprint 12:

* Real-corpus retraining (CourtListener tax-court rows). Still gated on
  enough Layer-3 enrichment for outcomes — the layer-3 worker exists
  but coverage is sparse. Sprint 13 candidate.
* Cross-validation / hyperparameter sweep. We retrain with the same
  hyperparams the v0 script used and rely on Brier comparison.
* Real ideology features as a model input. The model still consumes a
  scalar `ideologyDistance`; the operator (or the resolver) supplies it.

---

## Tickets

### S12.1 — Plan doc (this file)

### S12.2 — Variance sweep against the live v0 model

* New script `scripts/model-variance-sweep.sh` runs ~20 GraphQL
  predictions with deliberately-varied inputs and writes the resulting
  pWin distribution to stdout + a JSON report.
* PASS criterion: `max(pWin) - min(pWin) >= 0.10`. If false, the
  jurisdiction fix didn't unstick the model and Sprint 12 is just the
  retrain.

### S12.3 — `synthetic_cases_v1.parquet`

* Updated `scripts/gen_synthetic.py` (or new `gen_synthetic_v1.py`)
  produces 2000 rows with the existing schema:
  ```
  judge_severity, attorney_win_rate, ideology_distance,
  materiality_score, procedural_motion_count, case_type, jurisdiction, outcome
  ```
* Outcome logic: a logistic combiner over the features with realistic
  weights (positive on attorney_win_rate, negative on judge_severity,
  small non-monotonic on ideology_distance, jurisdiction multiplier).
  Adds noise so the model has signal AND uncertainty to learn.

### S12.4 — Retrain champion on v1

* Run `python scripts/train_first_models.py --data data/synthetic_cases_v1.parquet`
  (existing script — no code change needed).
* Verify the new champion's `bio.mqs.run_id` etc. is wired correctly via
  the predict.py loader (Sprint 6 fixed the MLflow 3 paths).
* The gateway picks up the new champion automatically — `champion.json`
  is reread on every prediction (no cache).

### S12.5 — Update MODEL_CARD

* Update `python/ml-inference-svc/MODEL_CARD.md` with v1 metrics.
* Honest disclosure: 2000-row synthetic corpus, no party features,
  Tier-A-only allowlist, known limitations.

### S12.6 — Replace Beta banner

* Replace the amber "Beta model" banner on `/cases` with an info-tone
  banner that names the current model version + dataset and links to
  MODEL_CARD. Drop the "Sprint 11" tracking reference (we're past it).

### S12.7 — Smoke + commit + push

* `scripts/model-variance-sweep.sh` PASSES against v1.
* All earlier smokes (DIME / MQ / JCS / provenance / date-filed) still
  PASS.
* Commit + push.

---

## Out of scope (Sprint 13+)

* **S13.x — Real-corpus retrain** (CourtListener tax cases with derived
  features + outcomes; depends on layer-3 enrichment coverage).
* **S13.x — Cross-validation + hyperparameter sweep**.
* **S13.x — Attorney ideology features (state-bar + FEC name-matching).**
* **S14.x — Per-term JCS lookup.**
* **S14.x — Editing `date_filed` after createCase.**

---

## Risks

| Risk | Mitigation |
|---|---|
| Sweep finds variance is still ~0 — jurisdiction fix didn't help | The fix is correct semantically (gateway → ML now passes "Federal" not "us-federal"). If outputs are still flat, retrain is the cure either way; just drop S12.2 from the gating logic. |
| v1 synthetic data is "too clean" — Brier jumps but the model overfits | Add explicit noise in the synthetic generator; document the corpus's known-synthetic nature in MODEL_CARD. |
| Retrain produces a worse champion than v0 | Keep the v0 run_id in MLflow; the predict.py loader stays pinned to `champion.json` so we can revert by editing one file. |

---

## Definition of done

* Variance sweep shows >=10 pp spread on at least one ensemble configuration.
* `synthetic_cases_v1.parquet` exists in `data/`.
* `mlruns/champion.json` points at a v1-trained run.
* MODEL_CARD reflects v1 metrics.
* `/cases` banner is no longer the amber "Beta model" copy.
* All prior smokes still PASS.
