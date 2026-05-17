"""
Sprint 12 / S12.3 — synthetic case data v1.

The v0 generator (generate_synthetic_cases.py) samples features independently
of outcome — pure noise — so trained models collapse to ~50% / Settle
regardless of input (Sprint 12 audit confirmed: 2.2 pp pWin spread across
20 diverse inputs).  v1 fixes that:

  1. Sample features from realistic distributions.
  2. Build a logistic combiner: outcome = Bernoulli(sigmoid(W . features + bias))
     with weights chosen to mimic the spec's expected effects:
        + attorney_win_rate (strong positive — better attorneys win more)
        - judge_severity    (strong negative — severe judges rule against)
        - ideology_distance (mild negative — alignment helps)
        + materiality_score (mild positive — strong cases win more)
        - procedural_motion_count (mild negative — drag of motions)
        case_type / jurisdiction shift the bias slightly.
  3. Add Gaussian noise on the logit so the model has uncertainty to learn
     conformal CIs from, not a perfect deterministic boundary.

Same column schema as v0 so train_first_models.py works unchanged.

Usage:
    python generate_synthetic_cases_v1.py --seed 42 --output data/synthetic_cases_v1.parquet
"""
from __future__ import annotations

import argparse
import os
import numpy as np
import pandas as pd


# Coefficients per the docstring above. Tuned by hand so the variance
# sweep produces a spread of at least ~30 pp across diverse inputs while
# keeping the marginal Brier in a defensible range (0.18-0.22, not so
# overfit that ECE is suspicious).
W = {
    "attorney_win_rate":        3.0,
    "judge_severity":          -3.0,
    "ideology_distance":       -1.0,
    "materiality_score":        1.0,
    "procedural_motion_count": -0.05,   # raw integer 0..20
}
BIAS = -0.5    # respondent-friendly baseline (tax-court mirror)

CASE_TYPE_BIAS = {"civil": 0.0, "criminal": -0.2, "bankruptcy": -0.1}
# Federal slightly more rights-protective for petitioners, NJ neutral, CA slightly less.
JURISDICTION_BIAS = {"Federal": 0.2, "California": -0.1, "New_Jersey": 0.0}

NOISE_SD = 0.6   # Gaussian noise on the logit; controls residual uncertainty.


def main(seed: int, output: str, n_rows: int) -> None:
    rng = np.random.default_rng(seed)

    jurisdictions = list(JURISDICTION_BIAS.keys())
    case_types = list(CASE_TYPE_BIAS.keys())

    # Per-row feature draws.
    judge_severity = rng.uniform(0, 1, size=n_rows)
    attorney_win_rate = rng.uniform(0, 1, size=n_rows)
    ideology_distance = rng.uniform(0, 1, size=n_rows)
    materiality_score = rng.uniform(0, 1, size=n_rows)
    procedural_motion_count = rng.integers(0, 21, size=n_rows)
    case_type = rng.choice(case_types, size=n_rows)
    jurisdiction = rng.choice(jurisdictions, size=n_rows)

    # Logit combiner.
    logit = (
        BIAS
        + W["attorney_win_rate"]        * attorney_win_rate
        + W["judge_severity"]           * judge_severity
        + W["ideology_distance"]        * ideology_distance
        + W["materiality_score"]        * materiality_score
        + W["procedural_motion_count"]  * procedural_motion_count.astype(float)
        + np.vectorize(CASE_TYPE_BIAS.get)(case_type)
        + np.vectorize(JURISDICTION_BIAS.get)(jurisdiction)
        + rng.normal(0.0, NOISE_SD, size=n_rows)
    )
    p = 1.0 / (1.0 + np.exp(-logit))
    outcome = (rng.uniform(0, 1, size=n_rows) < p).astype(int)

    df = pd.DataFrame({
        "jurisdiction": jurisdiction,
        "case_type":    case_type,
        "judge_severity":          judge_severity,
        "attorney_win_rate":       attorney_win_rate,
        "ideology_distance":       ideology_distance,
        "materiality_score":       materiality_score,
        "procedural_motion_count": procedural_motion_count,
        "outcome":                 outcome,
    })
    os.makedirs(os.path.dirname(output), exist_ok=True)
    df.to_parquet(output, index=False)
    print(f"Generated {len(df)} rows -> {output}")
    print(f"Outcome balance: petitioner={df.outcome.mean():.3f}")
    print(f"Feature -> outcome correlations:")
    for col in ["judge_severity", "attorney_win_rate", "ideology_distance",
                "materiality_score", "procedural_motion_count"]:
        r = df[col].corr(df.outcome)
        print(f"  {col:28s} r={r:+.3f}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--n-rows", type=int, default=2000)
    parser.add_argument("--output", type=str, required=True)
    args = parser.parse_args()
    main(args.seed, args.output, args.n_rows)
