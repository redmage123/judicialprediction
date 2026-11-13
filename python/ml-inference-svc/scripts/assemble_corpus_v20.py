"""
S22.6b — assemble corpus v20 = federal v19 + NJ/CA state-court rows.

v19 is the 5,937-row federal corpus (legal-BERT embeddings + LLM posture +
practice_area). v20 layers in the state-court opinions ingested in S22.4 and
labeled in S22.5 (tier-1 outcome) + this sprint's LLM passes (posture +
practice_area).

State rows carry the features we can actually compute for them:
  * outcome           — from case_documents.outcome_for (petitioner=1,
                        respondent=0; split rows are DROPPED, matching v1).
  * jurisdiction      — "State-CA" / "State-NJ" (derived from court_id) so the
                        model gets an explicit jurisdiction signal.
  * case_type         — from case_documents.case_type (Rust extractor).
  * procedural_posture— tier-1 regex (state_posture_shard_*.parquet) with the
                        LLM cache overriding the `unknown` residuals.
  * practice_area     — merged tier-1 + tier-2 LLM shard caches.
  * emb_0..emb_767    — legal-BERT sidecar built by build_legal_embeddings.py
                        over data/state_labeled.parquet.
  * _citation_count / _text_length — from the DB row.

Federal-only numeric features (judge_severity, attorney_win_rate,
ideology_distance, materiality_score, procedural_motion_count) are NaN for
state rows; the gradient-boosted trainer handles missing values natively.
Categorical party-type columns get the explicit "unknown" bucket. Citation
sub-counts are 0 (state cites_json is not yet extracted — Sprint 23).

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/assemble_corpus_v20.py \
        --federal data/real_corpus_v19.parquet \
        --emb data/legal_bert_emb.npy \
        --emb-meta data/legal_bert_emb_meta.json \
        --pa-cache-glob 'data/practice_area_state_shard_*.json' \
        --posture-cache-glob 'data/posture_state_shard_*.json' \
        --posture-tier1-glob 'data/state_posture_shard_*.parquet' \
        --out data/real_corpus_v20.parquet
"""
from __future__ import annotations

import argparse
import glob
import json
import os
from pathlib import Path

import numpy as np
import pandas as pd
import psycopg2

EMB_DIM = 768
OUTCOME_MAP = {"petitioner": 1, "respondent": 0}  # split -> dropped

# court_id -> jurisdiction label fed to the trainer's one-hot.
COURT_JURISDICTION = {
    "cal": "State-CA",
    "calctapp": "State-CA",
    "calag": "State-CA",
    "nj": "State-NJ",
    "njsuperctappdiv": "State-NJ",
    "njtaxct": "State-NJ",
}


def _merge_caches(pattern: str) -> dict[str, str]:
    """Union of all per-shard JSON caches: opinion_id(str) -> label."""
    merged: dict[str, str] = {}
    for path in sorted(glob.glob(pattern)):
        merged.update(json.loads(Path(path).read_text()))
    return merged


def _merge_posture_tier1(pattern: str) -> dict[str, str]:
    """tier-1 posture from the shard parquets: opinion_id(str) -> posture."""
    out: dict[str, str] = {}
    for path in sorted(glob.glob(pattern)):
        df = pd.read_parquet(path)
        for oid, post in zip(df["_opinion_id"].astype(str), df["procedural_posture"]):
            out[oid] = post
    return out


def _fetch_state_meta() -> pd.DataFrame:
    conn = psycopg2.connect(
        host="127.0.0.1",
        port=int(os.environ.get("PGPORT", "5454")),
        dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
        user=os.environ.get("PGUSER", "judicialpredict"),
        password=os.environ["POSTGRES_PASSWORD"],
    )
    try:
        return pd.read_sql(
            """SELECT opinion_id,
                      court_id,
                      outcome_for,
                      case_type,
                      citation_count,
                      LENGTH(full_text_plain) AS text_length
               FROM case_documents
               WHERE court_id IN ('cal','calctapp','calag','nj','njsuperctappdiv','njtaxct')
                 AND outcome_for IS NOT NULL""",
            conn,
        )
    finally:
        conn.close()


def main(args: argparse.Namespace) -> None:
    fed = pd.read_parquet(args.federal).reset_index(drop=True)
    fed_cols = list(fed.columns)
    print(f"federal v19: {len(fed)} rows, {len(fed_cols)} cols")

    # --- embeddings sidecar (row-aligned to its own meta opinion_ids) ---
    emb = np.load(args.emb)
    meta = json.loads(Path(args.emb_meta).read_text())
    emb_ids = [int(x) for x in meta["opinion_ids"]]
    done = int(meta.get("done", len(emb_ids)))
    emb_by_oid = {emb_ids[i]: emb[i] for i in range(min(done, len(emb_ids)))}
    print(f"embeddings: {len(emb_by_oid)} vectors (dim {emb.shape[1]})")

    # --- merged label caches ---
    pa = _merge_caches(args.pa_cache_glob)
    posture_t1 = _merge_posture_tier1(args.posture_tier1_glob)
    posture_llm = _merge_caches(args.posture_cache_glob)
    print(f"practice_area cache: {len(pa)}  posture tier-1: {len(posture_t1)}  posture LLM: {len(posture_llm)}")

    meta_df = _fetch_state_meta()
    print(f"state labeled rows in DB: {len(meta_df)}")

    rows = []
    skipped_split = 0
    skipped_no_emb = 0
    for r in meta_df.itertuples(index=False):
        oid = int(r.opinion_id)
        outcome_raw = r.outcome_for
        if outcome_raw not in OUTCOME_MAP:
            skipped_split += 1
            continue
        vec = emb_by_oid.get(oid)
        if vec is None:
            skipped_no_emb += 1
            continue
        oid_s = str(oid)
        # posture: LLM answer wins over a tier-1 `unknown`; else tier-1.
        post = posture_t1.get(oid_s, "unknown")
        if post == "unknown":
            post = posture_llm.get(oid_s, "unknown")
        row = {
            "judge_severity": np.nan,
            "attorney_win_rate": np.nan,
            "ideology_distance": np.nan,
            "materiality_score": np.nan,
            "procedural_motion_count": np.nan,
            "case_type": r.case_type or "civil",
            "jurisdiction": COURT_JURISDICTION.get(r.court_id, "State"),
            "petitioner_type": "unknown",
            "respondent_type": "unknown",
            "pro_se": 0,
            "procedural_posture": post,
            "cite_total": 0,
            "cite_density": 0.0,
            "cite_scotus": 0,
            "cite_circuit": 0,
            "cite_district": 0,
            "cite_taxcourt": 0,
            "cite_admin": 0,
            "outcome": OUTCOME_MAP[outcome_raw],
            "_opinion_id": oid,
            "_court_id": r.court_id,
            "_raw_case_type": r.case_type,
            "_raw_outcome": outcome_raw,
            "_appointing_president": None,
            "_citation_count": int(r.citation_count or 0),
            "_text_length": int(r.text_length or 0),
            "practice_area": pa.get(oid_s, "unknown"),
        }
        for d in range(EMB_DIM):
            row[f"emb_{d}"] = float(vec[d])
        rows.append(row)

    state = pd.DataFrame(rows, columns=fed_cols)
    print(f"state rows assembled: {len(state)}  (dropped split={skipped_split}, no-emb={skipped_no_emb})")

    out = pd.concat([fed, state], ignore_index=True)
    out.to_parquet(args.out, index=False)
    print(f"\nwrote {args.out}: {len(out)} rows ({len(fed)} fed + {len(state)} state), {len(out.columns)} cols")
    print(f"outcome dist: {out['outcome'].value_counts().to_dict()}")
    print(f"jurisdiction dist: {out['jurisdiction'].value_counts().to_dict()}")
    print("practice_area dist (top 8):")
    print(out["practice_area"].value_counts().head(8).to_string())


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--federal", required=True, type=Path)
    p.add_argument("--emb", required=True, type=Path)
    p.add_argument("--emb-meta", required=True, type=Path)
    p.add_argument("--pa-cache-glob", required=True)
    p.add_argument("--posture-cache-glob", required=True)
    p.add_argument("--posture-tier1-glob", required=True)
    p.add_argument("--out", required=True, type=Path)
    main(p.parse_args())
