"""
Per-court isotonic calibration — Sprint 20.1.

The Sprint 19 baseline fit a single Platt scaler over all courts.  But
petitioner-win base rates differ dramatically per court (f3d ≈ 28%,
SCOTUS ≈ 60%), so the global calibrator is systematically biased on
each individual court.  Fitting one isotonic regression per court on
its own slice of the cal set fixes that.

Falls back to a global isotonic regression for courts with fewer than
`min_n` cal-set samples (cafc, bia, tax in the v10 corpus).  Also falls
back to global when `court_id` is missing at inference time so callers
that haven't been updated to pass court still get sensible
probabilities.

Wrapped around an already-trained (`PlattCalibratedModel` or
`StackedEnsemble`) champion — the wrapper is logged to MLflow as the
production model when its Brier beats the unwrapped champion's.
"""
from __future__ import annotations

import numpy as np
from sklearn.isotonic import IsotonicRegression


class PerCourtIsotonicCalibrator:
    """
    Fit one isotonic regression per court (where there's enough data)
    plus a global fallback.
    """

    def __init__(self, min_n: int = 50) -> None:
        self.min_n = min_n
        self.global_iso: IsotonicRegression | None = None
        self.per_court: dict[str, IsotonicRegression] = {}

    def fit(
        self,
        p: np.ndarray,
        y: np.ndarray,
        court_ids: np.ndarray,
    ) -> "PerCourtIsotonicCalibrator":
        # Global fallback first — every court that lacks min_n samples
        # routes here, so it must always exist.
        self.global_iso = IsotonicRegression(out_of_bounds="clip")
        self.global_iso.fit(p, y)

        for court in np.unique(court_ids):
            mask = court_ids == court
            if mask.sum() < self.min_n:
                continue
            iso = IsotonicRegression(out_of_bounds="clip")
            iso.fit(p[mask], y[mask])
            self.per_court[str(court)] = iso
        return self

    def transform(self, p: np.ndarray, court_ids: np.ndarray) -> np.ndarray:
        if self.global_iso is None:
            raise RuntimeError("PerCourtIsotonicCalibrator.fit() must be called before transform()")
        out = np.empty_like(p, dtype=float)
        for i, court in enumerate(court_ids):
            iso = self.per_court.get(str(court), self.global_iso)
            out[i] = float(iso.transform([p[i]])[0])
        return out

    def fitted_courts(self) -> list[str]:
        return list(self.per_court.keys())


class PerCourtCalibratedChampion:
    """
    Wraps an already-Platt-calibrated base model (or the stacker) and
    applies a `PerCourtIsotonicCalibrator` as the final calibration
    layer.

    `predict_proba(X, court_ids=None)` keeps sklearn-style compatibility:
    when `court_ids` is omitted (legacy callers, MLflow signature
    inference, smokes that haven't been updated yet) the per-court layer
    falls back to its global isotonic — still better than the inner
    model's raw Platt because isotonic on OOF preds usually wins on real
    data.
    """

    def __init__(self, inner, calibrator: PerCourtIsotonicCalibrator) -> None:
        self.inner = inner
        self.calibrator = calibrator

    def _raw_p1(self, X: np.ndarray) -> np.ndarray:
        return self.inner.predict_proba(X)[:, 1]

    def predict_proba(
        self,
        X: np.ndarray,
        court_ids: np.ndarray | None = None,
    ) -> np.ndarray:
        p_raw = self._raw_p1(X)
        if court_ids is None:
            # Fall back to the global isotonic when callers haven't
            # plumbed court_id through.
            assert self.calibrator.global_iso is not None
            p_cal = self.calibrator.global_iso.transform(p_raw)
        else:
            p_cal = self.calibrator.transform(p_raw, np.asarray(court_ids))
        return np.column_stack([1 - p_cal, p_cal])

    def predict(
        self,
        X: np.ndarray,
        court_ids: np.ndarray | None = None,
    ) -> np.ndarray:
        return (self.predict_proba(X, court_ids)[:, 1] >= 0.5).astype(int)

    def get_params(self, deep: bool = True) -> dict:
        return {}

    def set_params(self, **params) -> "PerCourtCalibratedChampion":
        return self
