# Sprint 17 — JudicialPredict

**Theme:** Fix the era mismatch surfaced by the Sprint 16 post-mortem and
finally promote a real-corpus champion. Sprint 14 → 16 plateaued at
Brier 0.22 across three retrains because we were training on 93%
pre-1900 SCOTUS — and modern legal practice has 2× stronger feature
correlation with outcomes than 18th-century doctrine. With the CAP
ingest ordered descending (commit `aa18c41`), a same-sized re-ingest
gives us modern volumes. The features and the four-model ensemble are
unchanged; only the training corpus shifts.

**Window:** 2026-05-18 → (1 week — single-ticket sprint).

---

## Carry-forward from Sprint 16

* Five Sprint-16 sub-tickets all landed (S16.2 / S16.3 / S16.4 / S16.5
  / S16.6 / S16.7). All four NEUTRAL_FILL-pinned features now have
  real signal:
  * `judge_severity`: 27% noisy → 59.8% precision-correct.
  * `attorney_win_rate`: 0% → 15.1%.
  * `ideology_distance`: 0% → 15.9%.
  * `materiality_score`: 0% → 100%.
* Retrain produced Brier 0.2209 (XGBoost), still failing the 0.18
  gate. Champion remains Sprint 12.5 LR.

The post-mortem diagnostic surfaced the actual blocker: era skew.

| Slice | n | judge_severity ↔ outcome r |
|---|---|---|
| Synthetic v1 (Sprint 12.5 champion) | 2000 | **−0.328** |
| Real, pre-1900 (Sprint 16 corpus) | 589 | −0.131 |
| Real, modern (2000+) | 41 | **−0.264** |

Modern correlation is 2× pre-1900 and within striking distance of
synthetic. Volume × signal makes the path obvious.

---

## Goals

1. **Append-only ingest of modern CAP volumes.** Sprint 16 grabbed
   the earliest 5,000 opinions because volumes were sorted ascending.
   The new descending order pulls the latest 5,000 per jurisdiction.
   Keep the existing 4,958 pre-1900 rows in the DB for completeness
   (we want a full era distribution for any future temporal slice
   analysis).
2. **Re-extract + rebuild parquet.** The detector + judge/attorney
   rollups are unchanged; we just need extract-features to process
   the new rows.
3. **Retrain the full four-model ensemble** on `real_corpus_v6`. Use
   the SAME training pipeline (`train_first_models.py`) that produced
   v3 / v5; no hyperparameter changes.
4. **Promotion gate.** Brier ≤ 0.18, ECE ≤ 0.08, variance ≥ 0.10 pp
   spread on the canonical 20-input probe. Source-stratified Brier
   parity (modern vs pre-1900) is a NEW criterion: the model must
   not perform > 50% worse on either era slice — if it does, train
   on the modern slice only and document why.

What we are **not** doing in Sprint 17:

* Era-slice training (modern-only). Reserved as a Sprint 17.1
  fallback if the mixed corpus regresses on the modern slice.
* Panel-mean weighting (one of the original Sprint 17 candidates) —
  defer to S18 if the mixed-era retrain promotes.
* Learned outcome classifier on SCDB labels — defer to S18.
* New CAP jurisdictions beyond `us`, `f3d`, `f4th` — these three
  give us ~15,000 modern opinions across SCOTUS + Federal Reporter
  (3d) + Federal Reporter (4th). Plenty for the gate.

---

## Tickets

### S17.1 — Plan doc (this file)

### S17.2 — Modern CAP ingest (in flight)

Background task already started:
```
DATABASE_URL=... rust/target/release/ingest-fetcher cap \
    --limit 5000 --jurisdictions us,f3d,f4th
```

Append-only via the existing `ON CONFLICT (opinion_id) DO NOTHING`.
Expected runtime ~25 minutes at the observed ~70 opinions/minute
rate.

Gate: ≥ 1,500 modern (post-2000) rows in `case_documents` after the
run completes. Stop the run early if it exceeds 10,000 total rows
to avoid disk pressure.

### S17.3 — Re-extract features

```
docker exec -i judicialpredict_postgres psql -U judicialpredict -d \
    judicialpredict_dev -c \
    "UPDATE case_documents SET features_extracted_at = NULL WHERE source='cap';"
rust/target/release/ingest-fetcher extract-features
```

Note: the UPDATE is scoped to `source='cap'` so the (small) CL slice
keeps its existing labels. We expect detector hit rate on modern
SCOTUS to be much higher than on early-CAP — modern syllabus
language is more standardized.

Gate: ≥ 500 hard binary labels on modern rows.

### S17.4 — real_corpus_v6 + retrain

Standard pipeline:
```
docker exec -i judicialpredict_postgres psql -tA -f \
    python/ml-inference-svc/scripts/export_real_corpus.sql > /tmp/v6.json
.venv/bin/python python/ml-inference-svc/scripts/build_real_corpus.py \
    --input /tmp/v6.cleaned.json \
    --output python/ml-inference-svc/data/real_corpus_v6.parquet
.venv/bin/python python/ml-inference-svc/scripts/train_first_models.py \
    --data python/ml-inference-svc/data/real_corpus_v6.parquet
```

### S17.5 — Promotion + MODEL_CARD

Promotion gate:
1. Brier ≤ 0.18.
2. ECE ≤ 0.08.
3. Variance sweep ≥ 0.10 pp spread.
4. Source-stratified Brier within 50% across modern vs pre-1900.

If all four pass: champion is the v6 winner. Update MODEL_CARD's
versioning table, predict.py picks the new champion via the
existing `_load_champion` reread.

If any fail: keep Sprint 12.5 LR, document the new gap, and either
slice to modern-only (S17.1 fallback) or punt to S18.

---

## Out of scope (Sprint 18 candidates)

* Panel-mean weighting for multi-judge SCOTUS opinions.
* Learned outcome classifier on SCDB-labelled SCOTUS.
* Expanded CAP jurisdictions (state courts, district reports).
* MLflow tracking-backend migration to sqlite/postgres.
* RECAP docket integration (procedural history feature).

---

## Risks

| Risk | Mitigation |
|---|---|
| Modern CAP volumes are 404 (we observed f3d/f4th 404s on the first try) | Sprint 15.5's CAP module already handles this gracefully — logs + continues. If both f3d and f4th 404, fall back to `us` only and re-evaluate corpus size. |
| Mixed-era training regresses on modern slice | S17.5's source-stratified Brier gate catches this. Fallback: train on modern slice only and document why. |
| Detector recall is still low on modern SCOTUS | The 0.18 Brier gate catches this. Fallback: SCDB-labelled training data only for SCOTUS (drops to a 9K-row SCOTUS-only subset; still 4x Sprint 16's labelled corpus). |
| Disk pressure | Cap CAP ingest at 5K per jurisdiction; current dev box has ~80 GB free. |

---

## Definition of done

* Modern CAP ingest run; ≥ 1,500 post-2000 rows in `case_documents`.
* Re-extracted features; ≥ 500 new hard binary labels.
* `data/real_corpus_v6.parquet` exists with ≥ 1,000 labelled rows.
* Full four-model ensemble + stacked blender trained on v6.
* Promotion gate evaluated; champion.json points to either v6 winner
  OR remains pinned to Sprint 12.5 with an explicit "Sprint 17 not
  promoted" note in MODEL_CARD.
* All prior smokes still PASS.
* CL daily backfill continues unchanged.
