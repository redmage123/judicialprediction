"""
S20.6 — promote a real-data champion run to production.

Reads a parquet (the corpus the champion was trained on) plus the
champion's run_id, then writes three artifacts that `predict.py` needs
to serve the model at inference time:

  mlruns/<exp>/<run>/artifacts/feature_contract.json
      Structured-feature column order + one-hot category orders +
      embedding column count. The contract is what predict.py uses to
      build the input vector from the operator-supplied feature dict.

  mlruns/<exp>/<run>/artifacts/structured_encoder.pkl
      Fitted sklearn OrdinalEncoder over the categorical columns. Same
      encoder train_first_models.py used during training, refit on the
      same df so the column→ordinal mapping matches.

  mlruns/champion.json
      Updated to point at the new run_id + carries an inline
      `feature_contract` snapshot so predict.py can decide what to do
      with the input dict before MLflow even loads.

Run with `--dry-run` to see the contract without writing anything.

Usage:
    .venv/bin/python scripts/promote_champion.py \
        --data data/real_corpus_v14.parquet \
        --run-id 8ba01003c252491eb5edb4c0138e11df \
        --model-name stacked_ensemble_per_court
"""
from __future__ import annotations

import argparse
import json
import os
import pickle
import sys
from pathlib import Path

import pandas as pd

# Make scripts/ importable so we can reuse the feature-resolver logic.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import train_first_models as tfm  # noqa: E402
from train_first_models import (  # noqa: E402
    _resolve_feature_cols, encode_features,
)


def find_run_artifacts_dir(project_root: Path, run_id: str) -> Path:
    mlruns = project_root / "mlruns"
    for exp in mlruns.iterdir():
        if not exp.is_dir():
            continue
        candidate = exp / run_id / "artifacts"
        if candidate.exists():
            return candidate
    raise FileNotFoundError(f"no artifacts dir found for run {run_id} under {mlruns}")


def main(
    data_path: Path,
    run_id: str,
    model_name: str,
    dry_run: bool,
) -> None:
    df = pd.read_parquet(data_path)
    cat_cols, num_cols, all_cols = _resolve_feature_cols(df)
    # train_first_models.encode_features reads module-level
    # CATEGORICAL_FEATURES / NUMERIC_FEATURES / FEATURE_COLS. Mirror
    # the mutation main() does so the encoder fits on the right columns.
    tfm.CATEGORICAL_FEATURES = cat_cols
    tfm.NUMERIC_FEATURES = num_cols
    tfm.FEATURE_COLS = all_cols
    # Fit the encoder on the FULL corpus so its category order is
    # deterministic and matches what the model saw at training time.
    # (train_first_models.py's encode_features fits on the full df too,
    # before the train/test split, so the encoder we produce here is
    # the same one the model implicitly used.)
    _X, encoder = encode_features(df)

    # Embedding column count — pulled from the dataframe, since the
    # number of dims depends on which sentence model was used.
    embedding_cols = sorted(c for c in df.columns if c.startswith("emb_"))

    # Category orders per categorical column — operator dicts can pass
    # strings, predict.py maps them to ordinals via this lookup.
    category_orders: dict[str, list[str]] = {}
    for i, col in enumerate(cat_cols):
        category_orders[col] = [str(v) for v in encoder.categories_[i]]

    contract = {
        "model_name": model_name,
        "run_id": run_id,
        "structured_features_order": all_cols,
        "categorical_features": cat_cols,
        "numeric_features": num_cols,
        "category_orders": category_orders,
        "embedding_dim": len(embedding_cols),
        "embedding_columns": embedding_cols,
        # Embedding inference contract — when embedding_dim > 0,
        # predict.py expects the operator to pass an `opinion_text`
        # string in the features dict. The 384-dim MiniLM vector is
        # computed at inference time and appended to the structured
        # vector in the order numeric→categorical→embedding (matching
        # train_first_models.encode_features + the embedding columns).
        "embedding_model": "sentence-transformers/all-MiniLM-L6-v2" if embedding_cols else None,
        "embedding_input_field": "opinion_text" if embedding_cols else None,
        "embedding_max_chars": 2000 if embedding_cols else None,
        "promoted_at": "2026-05-19",
        "promotion_note": (
            "Real-data champion replacing the synthetic Sprint 12.5 LR. "
            "Trained on real_corpus_v14.parquet (5,937 federal-circuit "
            "opinions, 5,198 f3d). Brier 0.1861 / ECE 0.0259."
        ),
    }

    print("=== feature contract ===")
    print(f"  model_name           : {model_name}")
    print(f"  run_id               : {run_id}")
    print(f"  numeric features ({len(num_cols)}): {num_cols[:10]}{' ...' if len(num_cols) > 10 else ''}")
    print(f"  categorical ({len(cat_cols)}): {cat_cols}")
    for c in cat_cols:
        print(f"    {c}: {category_orders[c]}")
    print(f"  embedding columns    : {len(embedding_cols)}")
    print(f"  embedding model      : {contract['embedding_model']}")
    if dry_run:
        print("\n--dry-run: nothing written")
        return

    here = Path(__file__).resolve().parent
    project_root = here.parent

    # Find the run's artifacts dir
    artifacts_dir = find_run_artifacts_dir(project_root, run_id)
    contract_path = artifacts_dir / "feature_contract.json"
    encoder_path = artifacts_dir / "structured_encoder.pkl"

    with open(contract_path, "w") as f:
        json.dump(contract, f, indent=2)
    print(f"\nwrote {contract_path}")

    with open(encoder_path, "wb") as f:
        pickle.dump(encoder, f)
    print(f"wrote {encoder_path}")

    # Update champion.json: keep the existing model_name/brier/ece/log_loss
    # if they were already written by the trainer, but also embed the
    # inline contract so predict.py can short-circuit without resolving
    # the artifact dir for the contract.
    champion_path = project_root / "mlruns" / "champion.json"
    if champion_path.exists():
        with open(champion_path) as f:
            existing = json.load(f)
    else:
        existing = {}
    # Read the metrics directly off mlruns
    metrics_dir = artifacts_dir.parent / "metrics"
    metric = {}
    for k, file_name in (
        ("brier", "brier_score"),
        ("ece", "ece"),
        ("log_loss", "log_loss"),
    ):
        mpath = metrics_dir / file_name
        if mpath.exists():
            line = mpath.read_text().strip().splitlines()[-1]
            metric[k] = float(line.split()[1])
    champion = {
        "model_name": model_name,
        "brier": metric.get("brier", existing.get("brier")),
        "ece": metric.get("ece", existing.get("ece")),
        "log_loss": metric.get("log_loss", existing.get("log_loss")),
        "run_id": run_id,
        "feature_contract": contract,
    }
    with open(champion_path, "w") as f:
        json.dump(champion, f, indent=2)
    print(f"wrote {champion_path}  (Brier={champion['brier']:.4f}, ECE={champion['ece']:.4f})")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", required=True, type=Path,
                        help="Parquet file used to train the champion")
    parser.add_argument("--run-id", required=True,
                        help="MLflow run_id of the champion")
    parser.add_argument("--model-name", required=True,
                        help="Champion model name (e.g. stacked_ensemble_per_court)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print the contract without writing")
    args = parser.parse_args()
    main(args.data, args.run_id, args.model_name, args.dry_run)
