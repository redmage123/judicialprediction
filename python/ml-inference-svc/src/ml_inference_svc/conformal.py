"""
Split-conformal predictor for JudicialPredict ML inference.

Implements the simplest split-conformal interval:
  1. Fit on a held-out calibration set to compute nonconformity scores.
  2. At inference, return p_win +/- quantile(scores, 1-alpha), clipped to [0,1].

Reference: Angelopoulos & Bates (2022) — A Gentle Introduction to Conformal Prediction.
"""
from __future__ import annotations

import numpy as np


class SplitConformalPredictor:
    """
    Marginal split-conformal predictor for binary probability outputs.

    Nonconformity score: |y_true - p_hat|  (absolute residual on cal set).
    Coverage guarantee: P(y in [lower, upper]) >= 1 - alpha (marginally).
    """

    def __init__(self) -> None:
        self._residuals: np.ndarray | None = None

    def fit(self, y_cal: np.ndarray, p_cal: np.ndarray) -> "SplitConformalPredictor":
        """
        Compute and store nonconformity scores from a calibration split.

        Args:
            y_cal: True binary labels (0/1) for the calibration set.
            p_cal: Model probability estimates for the calibration set.
        """
        self._residuals = np.abs(y_cal.astype(float) - p_cal)
        return self

    def predict_interval(
        self, p: float, alpha: float = 0.10
    ) -> tuple[float, float]:
        """
        Return a (1-alpha) conformal prediction interval around p.

        Args:
            p: Point-estimate probability from the model (0.0–1.0).
            alpha: Error level; 0.10 => 90 % coverage.

        Returns:
            (lower, upper) clipped to [0, 1].

        Raises:
            RuntimeError: If fit() has not been called yet.
        """
        if self._residuals is None:
            raise RuntimeError(
                "SplitConformalPredictor.fit() must be called before predict_interval()."
            )
        # Finite-sample-corrected quantile: ceil((n+1)(1-alpha))/n
        n = len(self._residuals)
        level = min(1.0, np.ceil((n + 1) * (1.0 - alpha)) / n)
        q = float(np.quantile(self._residuals, level))
        lower = float(np.clip(p - q, 0.0, 1.0))
        upper = float(np.clip(p + q, 0.0, 1.0))
        return lower, upper

    @classmethod
    def from_residuals(cls, residuals: np.ndarray) -> "SplitConformalPredictor":
        """Restore a predictor from a pre-computed residuals array (e.g. loaded from MLflow)."""
        obj = cls()
        obj._residuals = residuals
        return obj
