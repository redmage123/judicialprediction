"""
Synthetic case data generator for JudicialPredict.
Generates a Parquet file with balanced classes across jurisdiction, case_type, and outcome.
Usage: python generate_synthetic_cases.py --seed 42 --output data/synthetic_cases.parquet
"""
import argparse
import numpy as np
import pandas as pd
import os

def main(seed: int, output: str):
    rng = np.random.default_rng(seed)
    # Define possible values
    jurisdictions = ["Federal", "California", "New_Jersey"]
    case_types = ["civil", "criminal", "bankruptcy"]
    n_per_combo = 100  # 100 samples per (jurisdiction, case_type, outcome)
    rows = []
    for jurisdiction in jurisdictions:
        for case_type in case_types:
            for outcome in [0, 1]:
                for _ in range(n_per_combo):
                    row = {
                        "jurisdiction": jurisdiction,
                        "case_type": case_type,
                        "judge_severity": rng.uniform(0, 1),
                        "attorney_win_rate": rng.uniform(0, 1),
                        "ideology_distance": rng.uniform(0, 1),
                        "materiality_score": rng.uniform(0, 1),
                        "procedural_motion_count": rng.integers(0, 20),
                        "outcome": outcome,
                    }
                    rows.append(row)
    df = pd.DataFrame(rows)
    os.makedirs(os.path.dirname(output), exist_ok=True)
    df.to_parquet(output, index=False)
    print(f"Generated {len(df)} rows to {output}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--output", type=str, required=True)
    args = parser.parse_args()
    main(args.seed, args.output)
