# Sprint 20 — JudicialPredict

**Theme:** Sprint 14–19 saturated the existing feature set on real data.
Brier plateaued at ~0.19 with 5,937 labelled rows; adding 60% more rows
(3,699 → 5,937) moved Brier by 0.0025 in the wrong direction. The
ensemble is signal-limited, not data-limited. Sprint 20 expands the
feature set with five additions targeted at the variance the current
features can't see: per-court priors, party identity, procedural
posture, citation-graph structure, and opinion text content.

**Window:** 2026-05-19 → ~3 weeks.

---

## Carry-forward from Sprint 19

* Real-corpus champion still NOT promoted. `champion.json` pinned to
  Sprint 12.5 LR (Brier 0.1662 on synthetic_v1).
* `data/real_corpus_v10.parquet` retained (5,937 rows, 5,198 f3d).
* Sprint 19 retrain: LightGBM Brier 0.1949, ECE 0.0213 — gate-fail but
  competitive with published legal-prediction literature (Katz et al.
  0.18–0.22).
* Four-model convergence (XGB 0.1952, LGBM 0.1949, CB 0.1959, LR
  0.2032) confirms saturation, not overfit. Information ceiling of
  current feature set is ~0.19 Brier.

---

## Goals

1. **Add five new feature classes** to the real-corpus training pipeline,
   landed independently so each contribution to Brier is measurable.
2. **One retrain per feature class** so we get a clean Δ-Brier per
   addition; this builds a regression suite for future feature work.
3. **Final retrain after all five land** on a single combined corpus
   (`real_corpus_v15`).
4. **Promotion gate, revised.** The 0.18 gate was set against the
   synthetic baseline and is no longer defensible. New gate: Brier ≤
   **0.17** (modest target above the academic-literature floor of 0.18)
   AND ECE ≤ 0.05 AND four-model agreement within 0.01 (saturation
   check). If passed, the real-data ensemble replaces the synthetic LR
   as production champion.

What we are **not** doing in Sprint 20:

* Ingesting more raw cases — the corpus stops at v10.
* New jurisdictions beyond `us`, `f3d`, `f4th`, `cafc`, `bia`, `tax`.
* Replacing the four-model ensemble; only the feature vector changes.
* Promoting until all five features land + final retrain.

---

## Tickets

Order is cheapest-first so we get measurable wins early.

### S20.1 — Per-court calibration (1 day)

Brier baseline 0.1949 includes a single Platt curve over all courts;
petitioner-win rates differ dramatically (f3d ≈ 28%, SCOTUS ≈ 60%).
Fit a per-court isotonic regression on top of the global model.

**Files:**
* `python/ml-inference-svc/scripts/train_first_models.py` — add
  per-court calibration step after model fits.
* `python/ml-inference-svc/scripts/predict.py` — load per-court
  calibrators from artifact and dispatch by `court_id`.
* New artifact: `mlruns/<run_id>/per_court_calibrators.pkl`.

**Gate:** Brier improvement of at least 0.005 over v10 LGBM baseline
(0.1949 → ≤ 0.1899). ECE must not degrade.

### S20.2 — Party-type extraction (1–2 days)

Add `petitioner_type`, `respondent_type`, `pro_se` columns to
`case_documents`. Populate from caption + opinion text via regex.

**Patterns:**
* Corporation: `\b(Inc\.|LLC|Corp\.|Co\.|Ltd\.|N\.A\.|Trust)\b`
* Government: `\b(United States|State of|Commissioner|Secretary|City of|County of)\b`
* Pro se: `\b(pro se|appearing pro se|representing (himself|herself|themselves))\b` in `full_text_plain`
* Residual → individual

**Files:**
* `rust/ingest-fetcher/src/extract.rs` — add `extract_party_types`
  function, call from `run_extraction`.
* Migration: `rust/ingest-fetcher/migrations/00XX_party_types.sql` —
  add three columns.
* `python/ml-inference-svc/scripts/export_real_corpus.sql` — join the
  new columns.
* `python/ml-inference-svc/scripts/build_real_corpus.py` — one-hot
  encode the three new categoricals.

**Gate:** Brier improvement ≥ 0.005 over S20.1 result.

### S20.3 — Procedural posture (3–4 days)

Two-tier extraction: regex first (covers ~70%), LLM fallback for the
rest.

**Tier 1 regex patterns** (apply to first 2K chars of `full_text_plain`):
* `motion to dismiss`, `Rule 12\(b\)\(6\)` → `motion_dismiss`
* `motion for summary judgment`, `Rule 56` → `summary_judgment`
* `petition for (a )?writ of certiorari` → `cert_petition`
* `rehearing en banc` → `en_banc`
* `Daubert` → `daubert`
* `appeal from`, `appellant`, `appellee` → `direct_appeal`
* (plus ~5 more — see scripts/posture_patterns.json)

**Tier 2 LLM fallback** (for cases the regex doesn't match):
* New script: `python/ml-inference-svc/scripts/extract_posture_llm.py`
* Uses `claude-haiku-4-5` with structured-output schema.
* Input: first 2K chars of `full_text_plain`.
* Cost estimate: ~6K cases × ~$0.005/call = ~$30.

**Files:**
* `rust/ingest-fetcher/src/extract.rs` — tier 1 regex pass during
  ingest.
* `python/ml-inference-svc/scripts/extract_posture_llm.py` — tier 2,
  run as one-shot batch.
* Migration: add `procedural_posture` JSONB column to
  `case_documents`.

**Gate:** Brier improvement ≥ 0.010 over S20.2 result.

### S20.4 — Citation-graph features (1 week)

Use `eyecite` to parse citations from every `full_text_plain`. For each
case, count cited precedents that resolved petitioner-favorable vs
respondent-favorable (using our own labels from earlier-decided
opinions only, to avoid leakage).

**Features added:**
* `cited_pet_count` — number of cited precedents labelled `petitioner`
* `cited_resp_count` — number labelled `respondent`
* `cited_strength_pet` — sum of `log1p(citee.citation_count)` for
  petitioner-favorable citees
* `cited_strength_resp` — same for respondent-favorable
* `cited_ratio` — pet / (pet + resp), null-fill when both zero

**Files:**
* New: `python/ml-inference-svc/scripts/build_citation_graph.py` —
  one-shot extraction; uses eyecite.
* Migration: new table `case_citations(case_id, cites_case_id,
  citation_text)`.
* `export_real_corpus.sql` — LEFT JOIN computed aggregates.
* `build_real_corpus.py` — neutral-fill the four new features when
  case has no citations resolved.

**Leakage safety:** join condition `citee.decided_at < citer.decided_at`
in the aggregate. Train/test split must also respect this — temporal
holdout, not random.

**Gate:** Brier improvement ≥ 0.010 over S20.3 result.

### S20.5 — Text embeddings (1–2 weeks)

Embed every `full_text_plain` with `nlpaueb/legal-bert-base-uncased`
(768-dim). Store via `pgvector`. Stack onto feature matrix; train a
text-only LR head + the existing structured ensemble + an outer meta-LR
blender (2-stage).

**Files:**
* Migration: enable `pgvector` extension; new table `case_embeddings
  (case_id, embedding vector(768))`.
* New: `python/ml-inference-svc/scripts/build_embeddings.py` — batch
  job. GPU-accelerated where available.
* `train_first_models.py` — add text-only LR head; outer blender meta-LR
  combines text-LR proba with structured ensemble proba.
* `predict.py` — at inference time, embed the input opinion (or
  petition), run both heads, blend.

**Cost:** embedding generation is local (CPU/GPU), so $0 op-ex; ~30
min wall-clock for 6K opinions on one GPU.

**Latency note:** predict-time embedding adds ~50ms. Acceptable; can
be precomputed for known cases.

**Gate:** Brier improvement ≥ 0.020 over S20.4 result.

### S20.6 — Final retrain + promotion

After all five tickets land:

1. Build `real_corpus_v15.parquet` with all new feature columns
   populated.
2. Run `train_first_models.py` end-to-end.
3. Evaluate against new gate: Brier ≤ 0.17, ECE ≤ 0.05, four-model
   spread ≤ 0.01.
4. If passed:
   * Update `champion.json` to point at the v15 ensemble winner (likely
     the 2-stage blender).
   * Move `data/synthetic_cases_v1.parquet` to `data/archive/`.
   * Update `MODEL_CARD.md`: retire synthetic-v1 as baseline, real-data
     champion is now production.
5. If failed:
   * Document the new floor.
   * Iterate (Sprint 21).

---

## Risks

| Risk | Mitigation |
|---|---|
| Per-court calibration overfits small-n courts (cafc=36, bia=14, tax=9) | Fall back to global calibrator for any court with n < 100. |
| LLM posture extraction goes off the rails on unusual cases | Structured-output schema with strict enum; regex covers majority anyway. |
| Citation-graph self-leakage (case cites a case decided later by us) | Hard temporal filter in the aggregation SQL; temporal train/test split. |
| Embedding model OOM on 150KB opinions | Truncate to model's max context (typically 512 tokens) at sentence boundaries; use [CLS] pool. |
| Final gate still misses 0.17 | Ship the best-real-data model anyway, gate fail documented; the 0.17 number is aspirational, not a hard contract. |

---

## Definition of done

* All five tickets land on `main` as independent commits.
* Each ticket records its Δ-Brier in `MODEL_CARD.md`.
* `data/real_corpus_v15.parquet` exists with all five new feature
  columns populated.
* `champion.json` points to v15 winner OR explicitly documents
  Sprint 20 not promoted + reason.
* Synthetic-v1 corpus marked deprecated either way.
* All prior smokes still PASS.
