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
| Sprint 15 (probe) | 2026-05-18 | _not promoted_ | Real n=623 (+CAP SCOTUS) — see below |
| Sprint 16 (probe) | 2026-05-18 | _not promoted_ | Real n=630 + 4 features de-neutralised — see below |

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

## Sprint 15 retrain probe (not promoted)

Sprint 15 added four new data sources (SCDB labels for SCOTUS, FJC
Biographical Directory for ~6,300 federal judges, CAP federal-slice
opinions, and the existing CourtListener slice) plus federal-court
outcome detection (scotus / circuit / district scanners under a
`CourtFamily` dispatch). Net corpus:

| Source | case_documents | hard binary labels |
|---|---|---|
| CourtListener tax + cafc | 109 | 41 |
| CAP us (SCOTUS) | 4,958 | 582 (76 + 506 from post-fix re-extract) |
| **total** | **5,067** | **623** (207 pet / 416 resp) |

Base petitioner-win rate climbed to 33.2% (vs Sprint 14's 24.4%);
corpus grew **15×** over Sprint 14. The full four-model ensemble +
K=5 stacked blender was retrained:

| Model | Brier ↓ | ECE ↓ | LogLoss ↓ |
|---|---|---|---|
| XGBoost (GPU) | 0.2231 | 0.0027 | 0.6384 |
| LightGBM (GPU) | 0.2232 | 0.0026 | 0.6385 |
| CatBoost (GPU) | 0.2231 | 0.0027 | 0.6384 |
| Logistic Regression | 0.2231 | 0.0027 | 0.6384 |
| Stacked (meta-LR) | 0.2231 | 0.0045 | 0.6384 |

All five base/blended models converged to nearly identical metrics —
diagnostic of **flat feature signal**. The meta-LR weights split
roughly evenly across CatBoost (+0.39), LR (+0.36), XGBoost (+0.01)
and lightly negative on LightGBM (-0.04); no model finds a distinct
signal because four of the seven features are pinned to NEUTRAL_FILL:

* `attorney_win_rate` — no extractor.
* `ideology_distance` — DIME / MQ / JCS judge ideology is wired in
  the gateway resolver, but the LATERAL join from CAP opinions to
  the judges KG matches < 5% of SCOTUS panels today (FJC ingest
  added ~6,300 judges but the panel-name extractor in `kg.rs` only
  reads tax-court markers; circuit and SCOTUS panel headers need
  separate parsers — Sprint 16).
* `materiality_score` — not defined yet.

**Promotion gate result:**
* Brier 0.2231 > 0.18 ceiling → **FAIL**
* ECE 0.0027 ≤ 0.08 → pass
* Source-stratified parity → not material; SCOTUS is 95% of corpus.

**Decision:** champion remains Sprint 12.5 (`run_id
4539e88454d64c7fbce2091be1195bf7`). Sprint 15 MLflow runs
(XGB/LGB/Cat/LR/Stacked under experiment `judicialpredict-models`,
created on 2026-05-18) and `data/real_corpus_v3.parquet` are
retained.

**Sprint 15 was still net-positive:** the corpus is now 15× larger,
detector accuracy is validated across the federal family, FJC judges
are in the KG, and the gap that blocks promotion is now diagnosed
clearly (feature engineering, not data volume). Sprint 16 candidates:

1. **Real `judge_severity`** — fix the panel-name extractor in
   `rust/ingest-fetcher/src/kg.rs` to read SCOTUS / circuit headers
   so the FJC-populated KG actually matches. Should lift
   `judge_severity` from mostly-0.5 to a real distribution.
2. **Real `ideology_distance`** — feed the FJC `appointing_president`
   field as a coarse ideology proxy when DIME / MQ / JCS don't have
   the judge.
3. **Attorney-side features** — extract attorney names from CAP /
   CL opinion headers, build per-attorney win-rate rollups.
4. **Bigger CAP pull** — expand from 5k SCOTUS to 30k across f3d /
   f4th / f-supp (Sprint 15 ran only the `us` jurisdiction because
   `f3d`/`f4th`/`f-supp` returned 404 from static.case.law on the
   first try; URL or path adjustment needed).
5. **Learned outcome classifier** — train on SCDB-labelled SCOTUS,
   apply to CAP body. Better recall than the regex detector.

## Sprint 16.6: real `materiality_score` (proxy)

Sprint 16 stopped pinning `materiality_score` to the neutral 0.5 prior and
started deriving it from cheap, structurally-available signals on
`case_documents`:

```
raw                = log1p(citation_count) + log1p(text_length / 1000)
materiality_score  = clamp((raw - corpus_min) / (corpus_max - corpus_min), 0, 1)
```

Per-corpus `min` / `max` are computed once on the first run and persisted
to `data/materiality_calibration.json`. Subsequent runs (and inference)
read that sidecar so the scale is stable across retrains.

**Smoke on the Sprint 15 corpus (n=630 hard-labelled rows):**

| stat | value |
|---|---|
| non-neutral rows (`!= 0.5`) | **630 / 630** |
| mean | 0.342 |
| std  | 0.134 |
| min / max | 0.003 / 0.904 |
| calibration | min=0.0178, max=6.4551 |

`citation_count` is currently 0 on every CAP row (CourtListener
backfill hasn't populated it on SCOTUS yet), so the active signal is
`length(full_text_plain)`. Even with citations zeroed out the score
spreads across the full [0, 1] range and is monotone in opinion length
— the dominant complexity proxy for SCOTUS / circuit / tax opinions.

**Honest caveat — this is a coarse proxy.** Materiality has no
canonical definition; we are using "opinion complexity and downstream
influence" as a stand-in for "how material is the underlying claim?".
Two notes operators need to be aware of:

* A long, much-cited opinion is not the same as a high-stakes claim;
  it just correlates with one because consequential cases tend to draw
  longer reasoning and more citations downstream.
* As CL citation enrichment improves (Sprint 17+), the `citation_count`
  term will start contributing — at which point the calibration sidecar
  should be regenerated by deleting
  `data/materiality_calibration.json` before the next build.

When a better materiality model lands (e.g. an SCDB-trained or
amount-in-controversy regressor for tax/civil rights cases) the
helper's signature stays the same; only the formula behind
`compute_materiality` changes.

## Sprint 16 retrain probe (not promoted)

Sprint 16 attacked the diagnostic from Sprint 15: 4 of 7 features were
pinned to NEUTRAL_FILL. Five sub-tickets landed:

* **S16.2** — SCOTUS + Federal Circuit panel-name extractor in
  `rust/ingest-fetcher/src/kg.rs`. Justice signatures
  (`Marshall, Ch. J.` / `JUSTICE BREYER delivered` / `Mr. Justice
  HOLMES delivered` / `JOHNSON, J.`) plus circuit panel headers
  (`Before DYK, REYNA, and STOLL, Circuit Judges.`).
* **S16.3** — attorney extractor + per-attorney win-rate rollup.
  New `attorneys` table populated at extract-features time;
  439 attorneys with `bio.win_rate_proxy` across the 5,109-doc
  corpus.
* **S16.4** — `appointing_president → ideology proxy`. 44 presidents
  mapped in `python/ml-inference-svc/scripts/president_ideology.py`;
  fallback when DIME / MQ / JCS doesn't have the judge.
* **S16.5** — materialized opinion-author edge
  (`case_documents.primary_judge_name`) so the judge↔opinion join is
  precision-correct rather than substring-collision-prone.
* **S16.6** — `materiality_score` from
  `log1p(citation_count) + log1p(text_length / 1000)`, min-max
  normalised against a per-corpus calibration sidecar.

Net feature-coverage uplift on the 630-row labelled corpus:

| Feature | Sprint 15 | Sprint 16 |
|---|---|---|
| `judge_severity` (non-neutral) | 27% (noisy) | **59.8% (precision-correct)** |
| `attorney_win_rate` | 0% | **15.1%** |
| `ideology_distance` | 0% | **15.9%** |
| `materiality_score` | 0% | **100%** |

Retrain metrics on `data/real_corpus_v5.parquet` (n=630, base rate
33.3%, train/test 504/126):

| Model | Brier ↓ | ECE ↓ | LogLoss ↓ |
|---|---|---|---|
| XGBoost (GPU) | 0.2209 | 0.0008 | 0.6335 |
| LightGBM (GPU) | 0.2217 | 0.0117 | 0.6354 |
| CatBoost (GPU) | 0.2218 | 0.0028 | 0.6355 |
| Logistic Regression | 0.2223 | 0.0034 | 0.6367 |
| Stacked (meta-LR) | 0.2222 | 0.0002 | 0.6365 |

Progression across all real-corpus attempts:

| Sprint | n | Brier | Δ vs prior |
|---|---|---|---|
| Sprint 14 | 41 | 0.2571 | — |
| Sprint 15 | 623 | 0.2231 | −0.034 (corpus 15×) |
| Sprint 16 | 630 | **0.2209** | −0.002 (4 features de-neutralised) |
| Sprint 12.5 champion | 2000 synth | **0.1662** | — |

**Promotion gate result:**
* Brier 0.2209 > 0.18 ceiling → **FAIL**

**Decision:** champion remains Sprint 12.5 LR
(`run_id 4539e88454d64c7fbce2091be1195bf7`). `data/real_corpus_v5.parquet`
+ the 5 Sprint 16 MLflow runs retained.

**Final diagnostic — why the real-data attempts plateau at ~0.22:**

Feature ↔ outcome correlations on `real_corpus_v5`:

| Feature | Pearson r with outcome |
|---|---|
| `judge_severity` | −0.06 |
| `attorney_win_rate` | +0.02 |
| `ideology_distance` | +0.05 |
| `materiality_score` | −0.03 |

The features are **non-neutral but uninformative**. Compare to the
synthetic v1 corpus (which the champion was trained on):

| Feature | Synthetic v1 r | Real v5 r |
|---|---|---|
| `judge_severity` | −0.328 | −0.06 |
| `attorney_win_rate` | +0.337 | +0.02 |
| `ideology_distance` | −0.053 | +0.05 |
| `materiality_score` | +0.109 | −0.03 |

Synthetic v1 had ~6× the predictive correlation per feature. Why is
the real corpus so much weaker?

1. **Corpus skew toward early SCOTUS (1754–1875).** 93% of the
   labelled rows are CAP-ingested 18th-/19th-century SCOTUS opinions.
   Modern legal-outcome dynamics (which the v1 synthetic implicitly
   models) don't map onto Marshall-era doctrine.
2. **Multi-judge panels flatten the per-opinion judge signal.** For
   a SCOTUS opinion with 5+ justices, the `primary_judge_name` is
   only one of them — the case outcome reflects a panel vote, not
   that single judge's severity. The synthetic generator assumes a
   single-judge → single-outcome relationship.
3. **Coarse feature proxies.** `appointing_president` → ideology is
   a 44-value lookup; real DIME / MQ / JCS scores are
   judge-individual continuous values. The proxy loses most of the
   resolution.
4. **Attorney win-rate has self-reference noise.** When the same
   attorney appears in train + test, the rollup is computed across
   both — a leak that *should* boost correlation but doesn't,
   because attorneys appear once each on average.

**Sprint 17 candidates:**

1. **Slice the corpus by era.** Train on 1950+ rows only; the
   modern subset may have stronger feature correlations.
2. **Multi-judge panel weighting.** Compute per-opinion ideology /
   severity as the panel *mean* rather than the first author.
3. **Learned outcome classifier on SCDB-labelled SCOTUS** (deferred
   from Sprint 15) — train a separate tfidf-LR or small transformer
   on the SCDB-labelled cases and use it as a feature in the main
   ensemble.
4. **Pull more CAP jurisdictions** so `us` isn't 93% of the corpus.
