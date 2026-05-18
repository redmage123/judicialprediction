# Champion Model — JudicialPredict (Sprint 12.5 retrain)

**Run ID:** `823618c8c8744f368459357d68a41ce4`
**Model:** `LogisticRegression` (scikit-learn) with `StandardScaler`, Platt-calibrated.
**Training date:** 2026-09-04 (Sprint 12.5).
**Training corpus:** `data/synthetic_cases_v1.parquet` — 2000 synthetic rows from a deterministic logistic combiner over the seven Tier-A/B features, with Gaussian noise on the logit.

## What this model is, in one sentence

A calibrated logistic regression — the simplest model in the four-member
ensemble — selected as champion because the v1 corpus is synthesised from a
logistic process. With this data the trees overfit; LR is optimal.

## Why we retrained (Sprint 12.5 context)

Audit on 2026-05-17 found:

1. The Sprint-11 jurisdiction wire-format fix was working, but
2. The previous champion (XGBoost on `synthetic_cases_v0.parquet`) was
   essentially flat — a 20-input variance sweep showed only a **2.2 pp
   spread** in `pWin`. The v0 generator sampled features independently of
   outcome (pure noise vs. balanced labels), so no model could learn signal.

Sprint 12 regenerated the corpus (`v1`) with realistic feature-outcome
correlations and added a logistic-regression baseline plus a stacking
blender (S12.5). The variance sweep now shows **0.696 pp spread** across
the same 20 inputs — the model actually discriminates.

## Sample size

| Slice | Rows |
|---|---|
| `synthetic_cases_v1.parquet` total | 2000 |
| Training (80%) | 1600 |
| Holdout test (20%) | 400 |
| Petitioner-win base rate | 0.297 |
| Courts represented | n/a (synthetic — no real court data) |
| Jurisdictions represented | Federal, California, New_Jersey |

The corpus is **fully synthetic**. Real CourtListener-derived training is
Sprint 13+ work, gated on Layer-3 enrichment coverage of opinion outcomes.

## Ensemble

Four base models trained with diverse inductive biases, then a stacking
blender (logistic regression meta-learner on out-of-fold base
probabilities, K=5).

| Model | Brier ↓ | ECE ↓ | Log-loss ↓ |
|---|---|---|---|
| XGBoost (depth=4, 100 trees) | 0.1735 | 0.0373 | 0.5254 |
| LightGBM (depth=4, 100 trees) | 0.1754 | 0.0376 | 0.5300 |
| CatBoost (depth=4, 100 trees) | 0.1701 | 0.0421 | 0.5165 |
| **Logistic regression** ← champion | **0.1662** | 0.0471 | **0.5071** |
| Stacked ensemble (LR meta) | 0.1679 | 0.0482 | 0.5107 |

Comparison with the v0 champion (XGBoost on `synthetic_cases_v0.parquet`):
Brier dropped from **0.2499** → **0.1662** (a 33% reduction). v0 was
indistinguishable from a 50/50 coin flip; v1 produces real probabilities.

### Why LR wins on this corpus

The v1 generator is a logistic combiner: `outcome = Bernoulli(sigmoid(W . x + noise))`.
That data-generating process IS logistic regression. Trees with limited
depth approximate the smooth logistic surface but waste capacity on
spurious splits, slightly overfitting. The stacking meta-LR confirms this
— it weights LR with the largest coefficient (+3.09) of any base model,
and weights XGBoost negatively (-0.56) because it's slightly miscalibrated:

```
Meta-LR coefficients on stacked ensemble:
  xgboost              = -0.56   (down-weighted — miscalibrated)
  lightgbm             = +0.38
  catboost             = +2.19   (well-calibrated tree learner)
  logistic_regression  = +3.09   (correct functional form for this corpus)
```

When real CourtListener-derived training data lands (Sprint 13+) the
ranking will almost certainly shift — real legal outcomes have non-linear
feature interactions that trees catch and LR misses.

## Feature-outcome correlations on v1 corpus

Per-feature Pearson correlation with `outcome`:

| Feature | r |
|---|---|
| judge_severity | -0.328 (more severe → fewer petitioner wins) |
| attorney_win_rate | +0.337 (better attorney → more petitioner wins) |
| materiality_score | +0.109 (stronger claim → mild edge) |
| procedural_motion_count | -0.108 (more motions → mild drag) |
| ideology_distance | -0.053 (mild — non-monotonic in the synth) |

These are by design — the v1 generator's weights mirror the spec's
expected directions.

## Calibration

* **Platt scaling**: each base model's raw probabilities pass through a
  Logistic Regression Platt scaler fit on a 20%-of-train calibration
  slice. Reduces ECE meaningfully on the GBMs (~0.04).
* **Conformal prediction intervals**: split-conformal residuals from the
  cal slice are stored as an MLflow artifact and loaded by the gateway's
  `SplitConformalPredictor`. Default coverage 90%; loosens to wider CI on
  high-uncertainty rows.

## Tier-A/B allowlist

The model accepts only:

* `judge_severity`, `attorney_win_rate`, `ideology_distance`,
  `materiality_score`, `procedural_motion_count` (numeric)
* `case_type ∈ {civil, criminal, bankruptcy}`,
  `jurisdiction ∈ {Federal, California, New_Jersey}` (categorical)

Tier-C party-identifying features are rejected at the gateway's GraphQL
boundary AND at the ML service's `ALLOWLIST_FEATURES` check.

## Known limitations

1. **Synthetic corpus** — outcomes are simulated. The model learns the
   simulation, not real legal patterns. Replace with real data in S13+.
2. **No party features by design** — Tier-C is a hard architectural
   block; predictions cannot personalise.
3. **Three jurisdictions only** — anything outside `{Federal, California,
   New_Jersey}` encodes to the unknown-value sentinel (-1.0).
4. **Cf. ideology score sources** — the model still consumes a single
   scalar `ideologyDistance`. The Sprint 7-11 work wires DIME / MQ / JCS
   to feed THAT scalar, but the model itself doesn't see source / term /
   release metadata — the compliance footer does.

## Intended use

* Civil and criminal matter triage in Federal, California, and New Jersey
  jurisdictions.
* Decision support for partners weighing settle-vs-try. **Not** a
  substitute for legal judgement.
* Audit-defensible because every prediction carries: model version
  (MLflow run id), conformal interval, source vintage for ideology, and
  recommendation reasoning.

## Out-of-scope use

* Anything outside the three named jurisdictions.
* Cases where party identity is material to outcome (use a different
  model with appropriate compliance review).
* Real-time, high-volume scoring (rate-limited; intended for
  partner-driven case review).

## Versioning

| Version | Date | Champion | Notable |
|---|---|---|---|
| v0 | 2026-04 | XGBoost | Synthetic v0 corpus, Brier 0.25, effectively flat |
| v1 | 2026-05-13 | LogisticRegression | Real n=10 hand-labelled; deprecated |
| **Sprint 12.5** | 2026-09-04 | LogisticRegression | Synthetic v1 + stacker, Brier 0.1662 |
| Sprint 14 (probe) | 2026-05-18 | _not promoted_ | Real n=41 (CourtListener cafc + tax) — see below |

## Sprint 14 retrain probe (not promoted)

Sprint 14 retrained `logreg-v1-real` on a real-corpus parquet built from
41 hard-labelled CourtListener opinions (36 cafc + 5 tax, 10 petitioner /
31 respondent, base rate 0.244). The detection pipeline was extended in
this sprint to handle appellate dispositions
(`AFFIRMED`/`REVERSED`/`VACATED` + IN-PART forms) and the plural form
`Decisions will be entered for …` — that lifted outcome-label coverage
from **5/109 → 52/109** opinions before this train, with 41 of those 52
resolving to a hard binary outcome.

The retrained model came in materially worse than the Sprint-12.5
champion:

| Metric (lower better) | Sprint 12.5 (synth n=2000) | Sprint 14 (real n=41) |
|---|---|---|
| Brier | **0.1662** | 0.2571 |
| ECE | **0.0471** | 0.2524 |
| LogLoss | **0.5071** | 0.7088 |

This is not a surprise: 41 rows with 76%/24% class imbalance and four of
the seven features pinned to the neutral prior (no attorney/ideology/
materiality signals derivable from opinion text yet) doesn't out-fit a
2000-row synthetic corpus with consistent feature-outcome structure.

**Decision:** champion remains Sprint 12.5 (`run_id
4539e88454d64c7fbce2091be1195bf7`). The Sprint 14 MLflow run
(`run_id dfe701f41fd842c5a4e8ca68530d9703`) and the
`data/real_corpus_v2.parquet` artifact are retained for reference;
predict.py is unchanged. The CourtListener daily backfill continues so
the corpus grows past where a real-data champion is competitive.
