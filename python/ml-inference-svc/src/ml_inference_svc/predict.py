"""
Inference pipeline for JudicialPredict ml-inference-svc.

predict_case_outcome(features) -> (p_win, ci_lower, ci_upper, model_version)

Loads the champion model + the feature contract emitted by
`scripts/promote_champion.py`. The contract pins:
  * the order of structured features (numeric + categorical), so the
    inference-time encoder matches the trainer's encoder bit-for-bit;
  * the category orders for the OrdinalEncoder, so 'individual' →
    ordinal 2 here and 2 there;
  * the embedding model id + dim, so text-conditional inference (S20.5
    champion) lazy-loads MiniLM and embeds the operator-supplied
    `opinion_text` at call time.

Older synthetic-baseline champions (Sprint 12.5 LR) don't carry a
feature contract; the loader falls through to the legacy
`FEATURE_ORDER` path so those checkpoints can still serve traffic if
ever rolled back to.
"""
from __future__ import annotations

import json
import os
import pickle
from functools import lru_cache

import mlflow
import numpy as np

from ml_inference_svc.conformal import SplitConformalPredictor

# Legacy contract — kept for the Sprint 12.5 synthetic-v1 champion and
# any pre-S20.6 checkpoint that doesn't carry a feature_contract block.
FEATURE_ORDER = [
    "judge_severity",
    "attorney_win_rate",
    "ideology_distance",
    "materiality_score",
    "procedural_motion_count",
    "case_type",
    "jurisdiction",
]
_LEGACY_CASE_TYPE_MAP = {"civil": 0.0, "criminal": 1.0, "bankruptcy": 2.0}
_LEGACY_JURISDICTION_MAP = {"California": 0.0, "Federal": 1.0, "New_Jersey": 2.0}

# Operator-facing input keys allowed on the wire. Includes `court_id`
# for per-court isotonic routing (S20.1) and `opinion_text` for text-
# conditional inference (S20.5).
ALLOWLIST_FEATURES: frozenset[str] = frozenset(FEATURE_ORDER) | frozenset({
    "court_id",
    "opinion_text",
    # S20.2 — party-type features, present on real-data v11+ champions.
    "petitioner_type",
    "respondent_type",
    "pro_se",
    # S20.3 — procedural posture.
    "procedural_posture",
    # S20.4 — citation density features. Operators rarely supply these
    # directly; the API gateway computes them from `opinion_text` upstream.
    "cite_total", "cite_density", "cite_scotus", "cite_circuit",
    "cite_district", "cite_taxcourt", "cite_admin",
})


def _champion_meta() -> dict:
    here = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(os.path.dirname(here))
    meta_path = os.path.join(project_root, "mlruns", "champion.json")
    if not os.path.exists(meta_path):
        raise FileNotFoundError(
            f"Champion metadata not found at {meta_path}. "
            "Run train_first_models.py first."
        )
    with open(meta_path) as f:
        return json.load(f)


def _resolve_run_artifact_path(
    project_root: str, run_id: str, artifact_name: str
) -> str | None:
    mlruns_root = os.path.join(project_root, "mlruns")
    if not os.path.isdir(mlruns_root):
        return None
    for exp in os.listdir(mlruns_root):
        candidate = os.path.join(
            mlruns_root, exp, run_id, "artifacts", artifact_name
        )
        if os.path.isfile(candidate):
            return candidate
    return None


def _resolve_logged_model_path(project_root: str, run_id: str) -> str | None:
    mlruns_root = os.path.join(project_root, "mlruns")
    if not os.path.isdir(mlruns_root):
        return None
    for exp in os.listdir(mlruns_root):
        outputs_dir = os.path.join(mlruns_root, exp, run_id, "outputs")
        if not os.path.isdir(outputs_dir):
            continue
        for name in os.listdir(outputs_dir):
            if not name.startswith("m-"):
                continue
            artifacts = os.path.join(mlruns_root, exp, "models", name, "artifacts")
            if os.path.isfile(os.path.join(artifacts, "MLmodel")):
                return artifacts
    return None


@lru_cache(maxsize=1)
def _load_embedding_model():
    """Lazy import; the sentence-transformers/torch bundle is heavy."""
    os.environ.setdefault("TRANSFORMERS_NO_ADVISORY_WARNINGS", "1")
    os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")
    from sentence_transformers import SentenceTransformer
    return SentenceTransformer("sentence-transformers/all-MiniLM-L6-v2")


@lru_cache(maxsize=1)
def _load_champion():
    """Load champion model + contract + encoder + conformal predictor."""
    meta = _champion_meta()
    run_id = meta["run_id"]

    here = os.path.dirname(os.path.abspath(__file__))
    project_root = os.path.dirname(os.path.dirname(here))
    tracking_uri = "file://" + os.path.join(project_root, "mlruns")

    mlflow.set_tracking_uri(tracking_uri)
    model_path = _resolve_logged_model_path(project_root, run_id)
    model_uri = model_path if model_path else f"runs:/{run_id}/model"
    model = mlflow.sklearn.load_model(model_uri)

    # Feature contract — either inline on champion.json (post-S20.6) or
    # resolved from the run's artifacts dir.
    contract = meta.get("feature_contract")
    if contract is None:
        contract_path = _resolve_run_artifact_path(
            project_root, run_id, "feature_contract.json"
        )
        if contract_path is not None:
            with open(contract_path) as f:
                contract = json.load(f)

    # Encoder — only required when the contract has categoricals.
    encoder = None
    if contract and contract.get("categorical_features"):
        encoder_path = _resolve_run_artifact_path(
            project_root, run_id, "structured_encoder.pkl"
        )
        if encoder_path is not None:
            with open(encoder_path, "rb") as f:
                encoder = pickle.load(f)

    # Conformal residuals
    residuals_path = _resolve_run_artifact_path(
        project_root, run_id, "conformal_residuals.npy"
    )
    if residuals_path is None:
        raise FileNotFoundError(
            f"Conformal residuals artifact not found for run {run_id}. "
            "Re-run train_first_models.py."
        )
    residuals = np.load(residuals_path)
    conformal = SplitConformalPredictor.from_residuals(residuals)

    return model, conformal, meta, contract, encoder


def _encode_legacy(features: dict) -> np.ndarray:
    """Pre-S20.6 7-feature path — kept for backwards compat."""
    row = [
        float(features["judge_severity"]),
        float(features["attorney_win_rate"]),
        float(features["ideology_distance"]),
        float(features["materiality_score"]),
        float(features["procedural_motion_count"]),
        _LEGACY_CASE_TYPE_MAP.get(str(features["case_type"]), -1.0),
        _LEGACY_JURISDICTION_MAP.get(str(features["jurisdiction"]), -1.0),
    ]
    return np.array(row, dtype=float).reshape(1, -1)


def _encode_from_contract(
    features: dict, contract: dict, encoder
) -> np.ndarray:
    """
    Build the input vector following the contract's column order.
    The contract's `structured_features_order` is the EXACT column list
    the model was trained on — so for each column we look up:
      * if it's a base numeric feature → features dict
      * if it's a categorical → OrdinalEncoder
      * if it starts with `emb_` → the corresponding dim of the MiniLM
        vector computed from `opinion_text`

    Embeddings are computed once and indexed by column name (emb_NNN →
    vec[NNN]) so the inline insertion stays in lock-step with the
    trainer's column order.
    """
    cat_cols = contract["categorical_features"]
    structured_order = contract["structured_features_order"]
    emb_dim = contract.get("embedding_dim", 0)

    # Encode categoricals once via the trained OrdinalEncoder.
    cat_ordinal: dict[str, float] = {}
    if cat_cols and encoder is not None:
        cat_input = np.array(
            [[features.get(c, "") for c in cat_cols]], dtype=object
        )
        cat_encoded = encoder.transform(cat_input)[0]
        for col, val in zip(cat_cols, cat_encoded.tolist()):
            cat_ordinal[col] = float(val)

    # Compute the embedding vector once, then index by emb_NNN below.
    embedding_vec: np.ndarray | None = None
    if emb_dim and emb_dim > 0:
        opinion_text = features.get("opinion_text") or ""
        max_chars = contract.get("embedding_max_chars") or 2000
        text = opinion_text[:max_chars]
        if text.strip():
            model = _load_embedding_model()
            embedding_vec = model.encode(
                [text], convert_to_numpy=True,
            )[0].astype(np.float64)
        else:
            embedding_vec = np.zeros(emb_dim, dtype=np.float64)

    cat_set = set(cat_cols)
    row: list[float] = []
    for col in structured_order:
        if col.startswith("emb_"):
            if embedding_vec is None:
                row.append(0.0)
            else:
                idx = int(col.split("_", 1)[1])
                row.append(float(embedding_vec[idx]))
        elif col in cat_set:
            row.append(cat_ordinal.get(col, -1.0))
        else:
            # Base numeric — pull from features, neutral-fill 0.0 when
            # the operator omitted it. The gateway is responsible for
            # upstream defaults; this is the last-line-of-defense.
            val = features.get(col)
            row.append(float(val) if val is not None else 0.0)

    return np.array(row, dtype=float).reshape(1, -1)


def predict_case_outcome(
    features: dict,
    alpha: float = 0.10,
) -> tuple[float, float, float, str]:
    """
    Return (p_win, ci_lower, ci_upper, model_version) for a feature dict.

    Post-S20.6 champions carry a `feature_contract` that the loader
    uses to build the input vector. Pre-S20.6 champions fall through
    to the legacy 7-feature `_encode_legacy` path.

    Args:
        features: Dict mapping feature names to values. For the v14
                  champion, must contain the structured features named
                  in the contract plus `opinion_text` (raw text) to
                  drive MiniLM embedding. `court_id` is optional and
                  routes per-court isotonic calibration.
        alpha: Conformal error level (0.10 => 90 % CI).

    Returns:
        p_win, ci_lower, ci_upper, model_version (MLflow run_id).
    """
    model, conformal, meta, contract, encoder = _load_champion()

    if contract is not None:
        X = _encode_from_contract(features, contract, encoder)
    else:
        X = _encode_legacy(features)

    court_id = features.get("court_id")
    if court_id is not None:
        try:
            p_win = float(
                model.predict_proba(X, court_ids=np.array([str(court_id)]))[0, 1]
            )
        except TypeError:
            p_win = float(model.predict_proba(X)[0, 1])
    else:
        p_win = float(model.predict_proba(X)[0, 1])

    ci_lower, ci_upper = conformal.predict_interval(p_win, alpha=alpha)
    return p_win, ci_lower, ci_upper, meta["run_id"]
