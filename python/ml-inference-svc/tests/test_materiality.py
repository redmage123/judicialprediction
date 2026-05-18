"""
Unit tests for the S16.6 materiality_score helper.

The helper combines `citation_count` and `text_length` into a [0, 1]
importance proxy via a logged sum + per-corpus min-max normalisation.
These tests pin the contract: monotone, bounded, and graceful when the
calibration is degenerate (returns NEUTRAL_FILL).
"""
from __future__ import annotations

import json
import math
import os
import sys
from pathlib import Path

import pytest

# Allow running tests without the package installed.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))

from build_real_corpus import (  # noqa: E402
    NEUTRAL_FILL,
    compute_calibration,
    compute_materiality,
    load_or_build_calibration,
)


def test_low_end_is_zero_with_unit_calibration() -> None:
    # raw(0, 0) = log1p(0) + log1p(0) = 0  → maps to the low end.
    assert compute_materiality(0, 0, {"min": 0.0, "max": 1.0}) == 0.0


def test_high_inputs_push_above_neutral() -> None:
    # raw(100, 50000) = log1p(100) + log1p(50) ≈ 4.615 + 3.932 ≈ 8.55
    # Against a calibration spanning [0, 10] this is ~0.85 — well above 0.5.
    val = compute_materiality(100, 50000, {"min": 0.0, "max": 10.0})
    assert val > 0.5
    assert val <= 1.0


def test_returns_are_always_in_unit_interval() -> None:
    cal = {"min": 0.0, "max": 8.0}
    cases = [
        (0, 0),
        (1, 100),
        (10, 5000),
        (1_000, 1_000_000),
        # Far above the calibration ceiling — must clamp to 1.0, not overflow.
        (10_000, 10_000_000),
        # Negative / None-like inputs are coerced to 0.
        (-5, -10),
    ]
    for cc, tl in cases:
        val = compute_materiality(cc, tl, cal)
        assert 0.0 <= val <= 1.0, f"out of bounds: {val} for ({cc}, {tl})"


def test_degenerate_calibration_returns_neutral_fill() -> None:
    # min == max → no scale → fall back to NEUTRAL_FILL rather than blow up.
    assert compute_materiality(10, 5000, {"min": 5.0, "max": 5.0}) == NEUTRAL_FILL
    assert compute_materiality(10, 5000, {"min": 0.0, "max": 0.0}) == NEUTRAL_FILL


def test_empty_calibration_returns_neutral_fill() -> None:
    # Missing keys default to 0.0 / 0.0 → degenerate → neutral.
    assert compute_materiality(10, 5000, {}) == NEUTRAL_FILL


def test_monotone_in_each_input() -> None:
    cal = {"min": 0.0, "max": 10.0}
    # More citations at fixed length → not less material.
    a = compute_materiality(0, 5000, cal)
    b = compute_materiality(50, 5000, cal)
    assert b >= a
    # More text at fixed citations → not less material.
    c = compute_materiality(10, 1000, cal)
    d = compute_materiality(10, 100_000, cal)
    assert d >= c


def test_formula_matches_specification() -> None:
    # The contract is explicitly:
    #   raw = log1p(citation_count) + log1p(text_length / 1000)
    cal = {"min": 0.0, "max": 1.0}
    cc, tl = 7, 12345
    expected_raw = math.log1p(cc) + math.log1p(tl / 1000.0)
    # With min=0, max=1 the normalised value equals raw (clamped to 1).
    assert compute_materiality(cc, tl, cal) == min(1.0, expected_raw)


def test_compute_calibration_min_max() -> None:
    records = [
        {"citation_count": 0, "text_length": 0},
        {"citation_count": 100, "text_length": 50_000},
        {"citation_count": 5, "text_length": 5000},
        {"citation_count": None, "text_length": None},
    ]
    cal = compute_calibration(records)
    assert cal["min"] == 0.0
    # max is raw(100, 50000) = log1p(100) + log1p(50)
    expected_max = math.log1p(100) + math.log1p(50.0)
    assert cal["max"] == pytest.approx(expected_max, rel=1e-9)


def test_compute_calibration_empty_corpus() -> None:
    assert compute_calibration([]) == {"min": 0.0, "max": 0.0}


def test_load_or_build_calibration_round_trip(tmp_path: Path) -> None:
    sidecar = tmp_path / "materiality_calibration.json"
    records = [
        {"citation_count": 0, "text_length": 100},
        {"citation_count": 50, "text_length": 10_000},
    ]
    # First call computes + writes.
    cal_a = load_or_build_calibration(records, sidecar)
    assert sidecar.exists()
    persisted = json.loads(sidecar.read_text())
    assert persisted == pytest.approx(cal_a)
    # Second call reads back the same values (does not recompute on different
    # records — pin guarantees train/inference stability).
    cal_b = load_or_build_calibration([{"citation_count": 9999, "text_length": 0}], sidecar)
    assert cal_b == cal_a
