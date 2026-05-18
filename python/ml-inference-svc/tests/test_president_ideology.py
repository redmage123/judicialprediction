"""Unit tests for the S16.4 president-as-ideology proxy."""
from __future__ import annotations

import sys
from pathlib import Path

# Make the sibling `scripts/` package importable without a __init__ shim.
SCRIPTS_DIR = Path(__file__).resolve().parents[1] / "scripts"
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from president_ideology import (  # noqa: E402  — path tweak above
    PRESIDENT_IDEOLOGY,
    ideology_distance_from_president,
)


def test_trump_maps_to_positive_value():
    """A known conservative-appointing president has a positive scalar."""
    score = PRESIDENT_IDEOLOGY["Donald J. Trump"]
    assert score > 0, f"Trump should be positive, got {score}"
    assert -1.0 <= score <= 1.0


def test_obama_maps_to_negative_value():
    """A known liberal-appointing president has a negative scalar."""
    score = PRESIDENT_IDEOLOGY["Barack Obama"]
    assert score < 0, f"Obama should be negative, got {score}"
    assert -1.0 <= score <= 1.0


def test_unknown_president_returns_none():
    """The raw mapping returns None for presidents we did not enumerate.

    The corpus builder relies on this to fall through to NEUTRAL_FILL.
    """
    assert PRESIDENT_IDEOLOGY.get("President Snow") is None
    # The literal sentinel emitted by the FJC ingest for non-presidential
    # reassignments must never appear in the mapping — it would silently
    # contaminate the ideology feature.
    assert PRESIDENT_IDEOLOGY.get("None (reassignment)") is None


def test_helper_returns_neutral_for_none():
    """The helper folds None / unknown → 0.5 (NEUTRAL_FILL)."""
    assert ideology_distance_from_president(None) == 0.50
    assert ideology_distance_from_president("") == 0.50
    assert ideology_distance_from_president("President Snow") == 0.50


def test_helper_returns_neutral_for_reassignment_sentinel():
    """`None (reassignment)` is an FJC sentinel, not a real president —
    must fall through to NEUTRAL_FILL."""
    assert ideology_distance_from_president("None (reassignment)") == 0.50


def test_helper_returns_distance_for_known_president():
    """Helper returns |score| for mapped presidents, clamped to [0, 1]."""
    trump = ideology_distance_from_president("Donald J. Trump")
    obama = ideology_distance_from_president("Barack Obama")
    assert trump == abs(PRESIDENT_IDEOLOGY["Donald J. Trump"])
    assert obama == abs(PRESIDENT_IDEOLOGY["Barack Obama"])
    assert 0.0 <= trump <= 1.0
    assert 0.0 <= obama <= 1.0


def test_all_mapped_scores_in_valid_range():
    """Every entry in the mapping must be in [-1, +1]."""
    for name, score in PRESIDENT_IDEOLOGY.items():
        assert -1.0 <= score <= 1.0, f"{name}: {score} outside [-1, 1]"


def test_high_volume_presidents_are_mapped():
    """Presidents with ≥ 10 appointments in the dev DB must be mapped.

    Verified against `SELECT appointing_president, COUNT(*) FROM judges ...`
    on 2026-05-18.  If FJC ingest later adds a new president above this
    threshold, this test will fail and prompt a mapping update.
    """
    required = {
        "Donald J. Trump", "George W. Bush", "Ronald Reagan",
        "George H.W. Bush", "Gerald Ford", "Richard M. Nixon",
        "Dwight D. Eisenhower", "Joseph R. Biden", "Barack Obama",
        "William J. Clinton", "Jimmy Carter", "Lyndon B. Johnson",
        "John F. Kennedy", "Harry S Truman", "Franklin D. Roosevelt",
        "Calvin Coolidge", "Warren G. Harding", "Herbert Hoover",
        "William H. Taft", "Theodore Roosevelt", "Woodrow Wilson",
        "George Washington", "John Adams", "Thomas Jefferson",
        "James Madison", "James Monroe", "John Quincy Adams",
        "Andrew Jackson", "Martin Van Buren", "John Tyler",
        "James K. Polk", "Franklin Pierce", "James Buchanan",
        "Abraham Lincoln", "Andrew Johnson", "Ulysses Grant",
        "Rutherford B. Hayes", "Chester A. Arthur",
        "Grover Cleveland", "Benjamin Harrison", "William McKinley",
    }
    missing = required - PRESIDENT_IDEOLOGY.keys()
    assert not missing, f"Missing required president mappings: {missing}"
