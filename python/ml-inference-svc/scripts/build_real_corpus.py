"""
Build a real-corpus parquet for training v1.

Reads a JSON export of `case_documents` + joined `judges.bio.severity_proxy`
(see the matching SQL in `export_real_corpus.sql`) and projects each row onto
the 7 features the existing trainer expects:

    judge_severity, attorney_win_rate, ideology_distance,
    materiality_score, procedural_motion_count, case_type, jurisdiction

Plus the binary `outcome` target:
    outcome_for = 'petitioner' → 1  (petitioner / plaintiff wins)
    outcome_for = 'respondent' → 0  (respondent / IRS wins)
    outcome_for = 'split'       → DROPPED (binary classifier; v1 ignores)
    outcome_for IS NULL        → DROPPED (Rule 155 / unresolved)

Features we don't have real signals for in S5.7's extraction:
  * attorney_win_rate    — no attorney-side data; fill 0.50 (neutral prior)
  * ideology_distance    — no Martin-Quinn / ideology model; fill 0.50
  * materiality_score    — no materiality model; fill 0.50

The trainer (train_first_models.py) is unchanged.  This script just ships a
parquet it can read.  Honest framing of the v1 sample size lives in
MODEL_CARD.md, not here.

Usage:
    python scripts/build_real_corpus.py --input real_corpus.json --output data/real_corpus_v1.parquet
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

import pandas as pd

from president_ideology import ideology_distance_from_president

# ── Constants ────────────────────────────────────────────────────────────────

# Map S5.7 NLP outcome_for → binary outcome (1 = win for petitioner/plaintiff).
OUTCOME_MAP = {"petitioner": 1, "respondent": 0}

# Map CL court_id → jurisdiction string the trainer recognises.  The trainer
# expects exactly {"California", "Federal", "New_Jersey"} from the S0 synthetic
# corpus; current CL ingest is tax + scotus + cafc + bia, all federal.
JURISDICTION_MAP = {
    "tax": "Federal",
    "scotus": "Federal",
    "cafc": "Federal",
    "bia": "Federal",
}

# S5.7's case_type taxonomy is tax-specific; the trainer expects the broad
# civil/criminal/bankruptcy enum.  All tax-court matters are civil.
CASE_TYPE_MAP = {
    "income_tax": "civil",
    "innocent_spouse": "civil",
    "collection_due_process": "civil",
    "whistleblower": "civil",
    "estate_tax": "civil",
    "gift_tax": "civil",
    "partnership": "civil",
    "employment_tax": "civil",
    "penalty": "civil",
}

# Neutral prior for features we can't derive from opinion text alone.
NEUTRAL_FILL = 0.50

# Procedural motion count proxy — cheap regex.  Matches "motion for X",
# "motion to X", "Rule N motion".  Caps at 50 to match the form input limit.
MOTION_RE = re.compile(
    r"\bmotion(?:\s+(?:for|to)\b|\s+(?:was|is)\b)|Rule\s+\d+\s+motion",
    flags=re.IGNORECASE,
)


def count_motions(text: str | None) -> int:
    if not text:
        return 0
    return min(50, len(MOTION_RE.findall(text)))


def project_row(record: dict) -> dict | None:
    """Project one DB row to the trainer's feature dict.  Returns None when
    the row has no usable target (split / null)."""
    outcome_raw = record.get("outcome_for")
    if outcome_raw not in OUTCOME_MAP:
        return None

    court_id = record.get("court_id", "")
    case_type_raw = record.get("case_type", "")
    # judge_severity is NULL when no judge matched the opinion's LATERAL
    # join (especially common on cafc opinions where the panel names don't
    # appear in our small KG yet). Fall through to the neutral prior.
    severity_raw = record.get("judge_severity")
    if severity_raw is None:
        severity_raw = NEUTRAL_FILL
    # S16.4 — president-as-ideology fallback.  When the matched judge's row
    # has an appointing_president (every FJC Article III judge does), map
    # it through PRESIDENT_IDEOLOGY → |score|.  Unknown / missing presidents
    # fall back to NEUTRAL_FILL.
    ideology = ideology_distance_from_president(
        record.get("appointing_president"),
        neutral=NEUTRAL_FILL,
    )
    return {
        "judge_severity": float(severity_raw),
        "attorney_win_rate": NEUTRAL_FILL,
        "ideology_distance": float(ideology),
        "materiality_score": NEUTRAL_FILL,
        "procedural_motion_count": count_motions(record.get("full_text_plain")),
        "case_type": CASE_TYPE_MAP.get(case_type_raw, "civil"),
        "jurisdiction": JURISDICTION_MAP.get(court_id, "Federal"),
        "outcome": OUTCOME_MAP[outcome_raw],
        # Keep the raw fields around for auditability / debugging.
        "_opinion_id": record.get("opinion_id"),
        "_court_id": court_id,
        "_raw_case_type": case_type_raw,
        "_raw_outcome": outcome_raw,
        "_appointing_president": record.get("appointing_president"),
    }


def main(input_path: Path, output_path: Path) -> None:
    raw = json.loads(input_path.read_text())
    if not isinstance(raw, list):
        raise SystemExit("input must be a JSON array of rows")

    rows = [r for r in (project_row(rec) for rec in raw) if r is not None]
    if not rows:
        raise SystemExit("no usable rows — every row had outcome=split/null")

    df = pd.DataFrame(rows)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    df.to_parquet(output_path, index=False)

    win_rate = float(df["outcome"].mean())
    print(f"Wrote {len(df)} rows to {output_path}")
    print(f"Base win-rate (petitioner): {win_rate:.3f}")
    print(f"Court breakdown: {df['_court_id'].value_counts().to_dict()}")
    print(f"Outcome breakdown: {df['_raw_outcome'].value_counts().to_dict()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    main(args.input, args.output)
