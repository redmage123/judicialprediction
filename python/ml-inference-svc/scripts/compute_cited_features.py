"""
S22.3 — derive cited pet/resp-favored counts (temporal-safe).

For every opinion in the training corpus, walk `case_document_citations` to
its cited targets, join `case_documents.outcome_for`, and compute:

  cite_pet_favored   — # cited cases whose disposition was petitioner-favored
  cite_resp_favored  — # cited cases whose disposition was respondent-favored
  cite_outcomed_total — # cited cases that have ANY outcome label (denominator
                        for the ratio; avoids dividing by all-cited including
                        unoutcome-labeled cited targets)
  cite_pet_ratio     — cite_pet_favored / max(1, cite_outcomed_total)

Temporal safety: only count cited cases with `date_filed < citing.date_filed`.
Without this an opinion could be "informed" by the disposition of a case that
postdates it, which is leakage from the trainer's perspective.

Output: `data/cited_features.json`, a dict opinion_id (str) -> the four
metrics. The v19 corpus assembler folds these as new numeric columns. Rows
not present in the JSON (or with zero cited-with-outcome) are neutral-filled
to zero by the assembler — that matches `_encode_from_contract`'s missing-
numeric policy in predict.py.

Usage:
    POSTGRES_PASSWORD=... .venv/bin/python scripts/compute_cited_features.py \
        --corpus data/real_corpus_v18.parquet [--out data/cited_features.json]
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import pandas as pd
import psycopg2


_AGG_SQL = """
WITH citing_set AS (
    SELECT cd.opinion_id, cd.date_filed
    FROM case_documents cd
    WHERE cd.opinion_id = ANY(%s)
),
cited_with_outcome AS (
    SELECT
        cdc.citing_opinion_id,
        cd_cited.outcome_for,
        cd_cited.date_filed AS cited_date_filed
    FROM case_document_citations cdc
    JOIN case_documents cd_cited
      ON cd_cited.opinion_id = cdc.cited_opinion_id
    WHERE cd_cited.outcome_for IS NOT NULL
)
SELECT
    cs.opinion_id,
    COUNT(*) FILTER (
        WHERE co.outcome_for = 'petitioner'
          AND co.cited_date_filed < cs.date_filed
    ) AS cite_pet_favored,
    COUNT(*) FILTER (
        WHERE co.outcome_for = 'respondent'
          AND co.cited_date_filed < cs.date_filed
    ) AS cite_resp_favored,
    COUNT(*) FILTER (
        WHERE co.outcome_for IS NOT NULL
          AND co.cited_date_filed < cs.date_filed
    ) AS cite_outcomed_total
FROM citing_set cs
LEFT JOIN cited_with_outcome co
  ON co.citing_opinion_id = cs.opinion_id
GROUP BY cs.opinion_id;
"""


def main(corpus: Path, out_path: Path) -> None:
    df = pd.read_parquet(corpus)
    ids = df["_opinion_id"].astype("int64").tolist()

    conn = psycopg2.connect(
        host="127.0.0.1",
        port=int(os.environ.get("PGPORT", "5454")),
        dbname=os.environ.get("PGDATABASE", "judicialpredict_dev"),
        user=os.environ.get("PGUSER", "judicialpredict"),
        password=os.environ["POSTGRES_PASSWORD"],
    )
    try:
        with conn.cursor() as cur:
            cur.execute(_AGG_SQL, (ids,))
            rows = cur.fetchall()
    finally:
        conn.close()

    out: dict[str, dict[str, float]] = {}
    n_any = 0
    n_pet_only = n_resp_only = n_both = 0
    for opinion_id, pet, resp, total in rows:
        total = int(total or 0)
        pet = int(pet or 0)
        resp = int(resp or 0)
        if total == 0 and pet == 0 and resp == 0:
            continue  # skip neutral-fill rows; assembler will zero-fill
        ratio = pet / total if total > 0 else 0.0
        out[str(int(opinion_id))] = {
            "cite_pet_favored": float(pet),
            "cite_resp_favored": float(resp),
            "cite_outcomed_total": float(total),
            "cite_pet_ratio": float(ratio),
        }
        n_any += 1
        if pet > 0 and resp > 0:
            n_both += 1
        elif pet > 0:
            n_pet_only += 1
        elif resp > 0:
            n_resp_only += 1

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, indent=2))

    n_rows = len(df)
    print(f"corpus rows                : {n_rows}")
    print(f"opinions with any cited-w/-outcome (temporal-safe): "
          f"{n_any} ({100*n_any/n_rows:.1f}%)")
    print(f"  pet-favored only         : {n_pet_only}")
    print(f"  resp-favored only        : {n_resp_only}")
    print(f"  mixed                    : {n_both}")
    if n_any > 0:
        pets = [v["cite_pet_favored"] for v in out.values()]
        resps = [v["cite_resp_favored"] for v in out.values()]
        ratios = [v["cite_pet_ratio"] for v in out.values()]
        print(f"  cite_pet_favored  mean   : {sum(pets)/len(pets):.2f}, max {max(pets):.0f}")
        print(f"  cite_resp_favored mean   : {sum(resps)/len(resps):.2f}, max {max(resps):.0f}")
        print(f"  cite_pet_ratio    mean   : {sum(ratios)/len(ratios):.3f}")
    print(f"\nwrote {out_path}")


if __name__ == "__main__":
    here = Path(__file__).resolve().parent
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", required=True, type=Path)
    p.add_argument("--out", type=Path,
                   default=here.parent / "data" / "cited_features.json")
    a = p.parse_args()
    main(a.corpus, a.out)
