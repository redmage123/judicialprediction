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
import math
import re
from pathlib import Path

import pandas as pd

from president_ideology import ideology_distance_from_president
from party_types import classify_party_types
from procedural_posture import classify_procedural_posture
from citation_features import extract_citation_features

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


# S16.3 — attorney win-rate signal. Pulled from the LATERAL join in
# export_real_corpus.sql against the `attorneys` table populated by
# rust/ingest-fetcher/src/extract.rs::run_extraction. When the opinion
# didn't match any attorney row (e.g. CAFC opinions where the lead
# attorney's name isn't in our KG yet, or pre-1900 CAP opinions with no
# counsel block), fall through to NEUTRAL_FILL so the column has no
# missing values in the parquet.
def attorney_win_rate_from_record(record: dict) -> float:
    raw = record.get("attorney_win_rate")
    if raw is None:
        return NEUTRAL_FILL
    try:
        return float(raw)
    except (TypeError, ValueError):
        return NEUTRAL_FILL


# ── S16.6: materiality_score ─────────────────────────────────────────────────
#
# Coarse proxy for "how material / influential is this opinion?".  We don't
# have a canonical materiality signal; we combine two cheap proxies that are
# real on most rows:
#   * citation_count — how many later opinions cite this one (sparse on CAP)
#   * length(full_text_plain) — text length as proxy for complexity
# raw = log1p(citation_count) + log1p(text_length / 1000)
# materiality_score = clamp((raw - corpus_min) / (corpus_max - corpus_min), 0, 1)
# Per-corpus min/max are persisted to data/materiality_calibration.json so
# rerun + inference use the same scale.

MATERIALITY_CALIBRATION_PATH = (
    Path(__file__).resolve().parent.parent / "data" / "materiality_calibration.json"
)


def _raw_materiality(citation_count: int, text_length: int) -> float:
    cc = max(0, int(citation_count or 0))
    tl = max(0, int(text_length or 0))
    return math.log1p(cc) + math.log1p(tl / 1000.0)


def compute_materiality(citation_count: int, text_length: int, calibration: dict) -> float:
    """
    Combine citation count + opinion length into a [0, 1] importance proxy.

    Calibration dict: {"min": float, "max": float} computed by min-max
    sweep over the corpus on first run; persisted to data/materiality_calibration.json
    so the same scale applies across reruns / inference.
    """
    raw = math.log1p(max(0, int(citation_count or 0))) + math.log1p(
        max(0, int(text_length or 0)) / 1000.0
    )
    lo = float(calibration.get("min", 0.0))
    hi = float(calibration.get("max", 0.0))
    if hi - lo < 1e-6:
        return NEUTRAL_FILL
    val = (raw - lo) / (hi - lo)
    return max(0.0, min(1.0, val))


def compute_calibration(records: list[dict]) -> dict:
    """Min-max sweep over the corpus to set the [0, 1] scale."""
    raws: list[float] = []
    for rec in records:
        raws.append(
            _raw_materiality(
                rec.get("citation_count") or 0,
                rec.get("text_length") or 0,
            )
        )
    if not raws:
        return {"min": 0.0, "max": 0.0}
    return {"min": float(min(raws)), "max": float(max(raws))}


def load_or_build_calibration(records: list[dict], path: Path) -> dict:
    """Read sidecar JSON if present; else compute from corpus and persist."""
    if path.exists():
        try:
            cal = json.loads(path.read_text())
            if "min" in cal and "max" in cal:
                return {"min": float(cal["min"]), "max": float(cal["max"])}
        except (json.JSONDecodeError, ValueError, TypeError):
            pass
    cal = compute_calibration(records)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(cal, indent=2) + "\n")
    return cal


def project_row(record: dict, materiality_calibration: dict | None = None) -> dict | None:
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
    # S16.6: materiality_score from citation_count + text_length, normalised
    # against the per-corpus calibration. When the caller hasn't supplied a
    # calibration we fall back to the neutral prior (legacy behaviour).
    if materiality_calibration is not None:
        materiality = compute_materiality(
            record.get("citation_count") or 0,
            record.get("text_length") or 0,
            materiality_calibration,
        )
    else:
        materiality = NEUTRAL_FILL
    # S20.2 — party-type extraction. Coarse three-way classification of
    # each side (individual / corporation / government) plus a pro-se
    # flag. Computed from `full_text_plain`'s caption head; rows whose
    # caption can't be parsed default to ("individual", "individual",
    # False) so the column has no missing values. See scripts/party_types.py.
    parties = classify_party_types(record.get("full_text_plain"))
    # S20.3 — procedural posture. Tier-1 regex over the first 2K chars
    # of the opinion; tier-2 LLM fallback wired separately for the
    # 'unknown' bucket (off the build hot path).
    posture = classify_procedural_posture(record.get("full_text_plain"))
    # S20.4 — citation density features. Eyecite outgoing-citation
    # extraction by reporter family. The richer pet/resp-favored
    # citation counts are deferred until we have a citation→opinion
    # lookup; this is the cheap signal we can ship today.
    cites = extract_citation_features(record.get("full_text_plain"))
    return {
        "judge_severity": float(severity_raw),
        "attorney_win_rate": attorney_win_rate_from_record(record),
        "ideology_distance": float(ideology),
        "materiality_score": materiality,
        "procedural_motion_count": count_motions(record.get("full_text_plain")),
        "case_type": CASE_TYPE_MAP.get(case_type_raw, "civil"),
        "jurisdiction": JURISDICTION_MAP.get(court_id, "Federal"),
        "petitioner_type": parties.petitioner,
        "respondent_type": parties.respondent,
        "pro_se": int(parties.pro_se),
        "procedural_posture": posture.label,
        "cite_total": cites.cite_total,
        "cite_density": cites.cite_density,
        "cite_scotus": cites.cite_scotus,
        "cite_circuit": cites.cite_circuit,
        "cite_district": cites.cite_district,
        "cite_taxcourt": cites.cite_taxcourt,
        "cite_admin": cites.cite_admin,
        "outcome": OUTCOME_MAP[outcome_raw],
        # Keep the raw fields around for auditability / debugging.
        "_opinion_id": record.get("opinion_id"),
        "_court_id": court_id,
        "_raw_case_type": case_type_raw,
        "_raw_outcome": outcome_raw,
        "_appointing_president": record.get("appointing_president"),
        # S16.6: raw inputs preserved for downstream auditing.
        "_citation_count": int(record.get("citation_count") or 0),
        "_text_length": int(record.get("text_length") or 0),
    }


def main(
    input_path: Path,
    output_path: Path,
    calibration_path: Path | None = None,
) -> None:
    raw = json.loads(input_path.read_text())
    if not isinstance(raw, list):
        raise SystemExit("input must be a JSON array of rows")

    # S16.6: load or compute the materiality calibration BEFORE projecting
    # rows. The sidecar pins the [0, 1] scale across reruns / inference.
    cal_path = calibration_path or MATERIALITY_CALIBRATION_PATH
    materiality_calibration = load_or_build_calibration(raw, cal_path)

    rows = [
        r
        for r in (project_row(rec, materiality_calibration) for rec in raw)
        if r is not None
    ]
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
    # S16.6: surface the calibration + materiality distribution for the
    # operator running the build.
    print(
        f"Materiality calibration: min={materiality_calibration['min']:.4f} "
        f"max={materiality_calibration['max']:.4f} (sidecar: {cal_path})"
    )
    non_neutral = int((df["materiality_score"] != NEUTRAL_FILL).sum())
    print(
        f"materiality_score non-neutral: {non_neutral}/{len(df)} "
        f"(mean={df['materiality_score'].mean():.3f})"
    )
    # S20.2 — party-type distribution. If everything's "individual" then
    # the caption regex didn't match — surface that loudly so we can
    # investigate before retraining.
    print(
        f"petitioner_type: {df['petitioner_type'].value_counts().to_dict()}"
    )
    print(
        f"respondent_type: {df['respondent_type'].value_counts().to_dict()}"
    )
    print(
        f"pro_se: {int(df['pro_se'].sum())}/{len(df)} "
        f"({df['pro_se'].mean():.1%})"
    )
    print(
        f"procedural_posture: {df['procedural_posture'].value_counts().to_dict()}"
    )
    # S20.4 — citation density summary
    if "cite_total" in df.columns:
        print(
            f"cite_total: mean={df['cite_total'].mean():.1f}, "
            f"median={df['cite_total'].median():.0f}, max={df['cite_total'].max()}, "
            f"zero-rate={(df['cite_total'] == 0).mean():.1%}"
        )
        print(
            f"cite per family — scotus_mean={df['cite_scotus'].mean():.1f}, "
            f"circuit_mean={df['cite_circuit'].mean():.1f}, "
            f"district_mean={df['cite_district'].mean():.1f}, "
            f"taxcourt_mean={df['cite_taxcourt'].mean():.1f}, "
            f"admin_mean={df['cite_admin'].mean():.1f}"
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--materiality-calibration",
        type=Path,
        default=None,
        help="Path to materiality calibration JSON sidecar (default: "
        "data/materiality_calibration.json next to this script).",
    )
    args = parser.parse_args()
    main(args.input, args.output, args.materiality_calibration)
