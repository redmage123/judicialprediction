# Champion Model — JudicialPredict v1 (real corpus)

**Run ID:** `9511b77a0c8145b8b5d6f16a7b0d1985`
**Model type:** `LogisticRegression` (scikit-learn) with class-balanced weights.
**Training date:** 2026-05-13.
**Sprint:** S6.1 (replaces the synthetic-data champion trained under S0).

## What this model is, in one sentence

A small-N, base-rate-anchored logistic regression on 10 hand-labelled real
opinions, intended as a defensible v1 — not a production-quality predictor.

## Sample size — the headline limitation

| Slice                   | Rows |
|-------------------------|------|
| `case_documents` total  | 99   |
| With `outcome_for IS NOT NULL` | 12 |
| Used in training (binary classifier; `split` excluded) | **10** |
| Petitioner wins (= 1)   | 2    |
| Respondent wins (= 0)   | 8    |
| Base rate (p_petitioner_wins) | **0.20** |
| Courts represented      | tax only |
| Jurisdictions represented | us-federal only |

The Sprint 5 risk plan
([SPRINT_5_PLAN.md](../../SPRINT_5_PLAN.md))
authorised this regime explicitly: "if at 5 days in we're below 500 rows,
train on what we have + flag in MODEL_CARD as v1, retrained when more data
accumulates." The
[courtlistener-daily ingest fix (`94b2590`)](../../rust/ingest-fetcher/src/rest.rs)
should produce 5–10 fresh labelled rows per week; the model retrains
automatically once the corpus crosses ~200 hard labels.

## Evaluation

Standard train/test holdout is uninformative on n=10 (test = 2 rows under a
20% split, of which 0 or 1 will be the minority class). We report
**leave-one-out cross-validation** instead.

| Metric         | Value   | Notes                                              |
|----------------|---------|----------------------------------------------------|
| Brier score    | 0.1832  | Lower is better; the constant-base-rate predictor scores 0.16 |
| ECE (5 bins)   | 0.2153  | Calibration is poor at n=10 — see caveats          |
| Log loss       | 0.5071  | —                                                  |

**Reading these honestly:** Brier is slightly *worse* than just predicting
the base rate (0.20) for everyone, because LogReg over-fits a 7-feature
boundary on 10 points and oscillates around the constant predictor. Once
the corpus crosses ~50 labelled rows the LogReg should beat the constant
baseline reliably; until then, callers should treat `p_win` as
"directionally informative" rather than "calibrated probability."

## Conformal interval (S6.2)

Split-conformal calibration uses the LOOCV residuals (n=10) directly as the
calibration set, with α = 0.10 → nominal 90% CI.

- **Empirical coverage at α=0.10:** 0.90 by construction of split-conformal
  (the residuals come from the same distribution as test predictions).
- **Typical CI width:** ~0.80 — i.e., for a borderline case the 90% CI
  is approximately `[0, 0.8]`. This is the honest "we don't know much"
  signal. The Sprint 6 plan relaxed the S5.2 target from ±2% to ±5%
  precisely to allow this.

## Features the model sees

The trainer projects each row onto the 7-feature schema
`ml_inference_svc.predict.FEATURE_ORDER`:

| Feature                     | Source                                            | v1 fidelity |
|-----------------------------|---------------------------------------------------|-------------|
| `judge_severity`            | `judges.bio.severity_proxy.severity` (S5.7) — fraction of this judge's prior decisions ruling against petitioner | Real signal (97/99 docs matched a judge) |
| `attorney_win_rate`         | Filled with 0.5 (neutral prior)                   | **Stub** — no attorney data in S6 |
| `ideology_distance`         | Filled with 0.5 (neutral prior)                   | **Stub** — no Martin-Quinn / ideology model |
| `materiality_score`         | Filled with 0.5 (neutral prior)                   | **Stub** — no materiality model |
| `procedural_motion_count`   | Regex over `full_text_plain` — "motion to/for/was/is", "Rule N motion" | Real proxy (clipped at 50) |
| `case_type`                 | S5.7 `case_type` → `civil` (every tax-court matter is civil) | Real (collapse-mapped) |
| `jurisdiction`              | Court slug → `Federal` for tax/scotus/cafc/bia    | Real (single-jurisdiction corpus) |

Three stub features means the model is essentially a 4-feature LogReg.

## What this model is NOT for

- Quoting probabilities to a client in a Settle vs Try memo — the CI is too
  wide and ECE is too high to make calibration claims.
- Cross-jurisdiction prediction — every training row is federal tax court.
- Sub-classifying petitioner-win cases by type — only 2 such rows exist.

## Retraining plan

- **Cadence:** retrain weekly while the corpus is growing (see
  `scripts/courtlistener-daily.sh` — adds 5–10 labelled rows per week).
- **Bar to graduate from v1:** ≥200 labelled rows, ≥3 jurisdictions
  represented, ECE ≤ 0.10 on a real 20% holdout. Sprint 6 plan tracks the
  growth; Sprint 7 likely cuts v2 if the corpus catches up.
- **Replacement:** when the bar is met, switch to the GBM ensemble path
  (`scripts/train_first_models.py`) which the small-N caveats currently
  block.

## Reproducing this run

```bash
# 1. Export labelled corpus from Postgres (see export_real_corpus.sql).
docker exec judicialpredict_postgres psql -U judicialpredict \
    -d judicialpredict_dev -tA -f /tmp/export_real_corpus.sql \
    > /tmp/real_corpus.json
# Trim the leading SET line (`tail -n +2 …`).

# 2. Build training parquet.
docker exec judicialpredict_ml_inference \
    python scripts/build_real_corpus.py \
    --input /tmp/real_corpus.json \
    --output data/real_corpus_v1.parquet

# 3. Train.
docker exec judicialpredict_ml_inference \
    python scripts/train_real_v1.py \
    --data data/real_corpus_v1.parquet \
    --mlruns-dir /tmp/mlruns

# 4. Move artefacts to the host mount + rewrite meta.yaml absolute paths.
#    (See SPRINT_6_NOTES.md or the S6.1 commit message for the exact dance.)
```
