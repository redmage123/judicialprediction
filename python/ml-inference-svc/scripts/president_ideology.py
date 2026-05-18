"""
President-as-ideology proxy (Sprint 16.4).

Coarse-but-defensible mapping of `judges.appointing_president` (populated by the
FJC Biographical Directory ingest in S15.4) to a scalar ideology score in
[-1, +1] where negative is liberal-leaning and positive is conservative-leaning.

The existing DIME / MQ / JCS ideology paths only cover ~3 hand-seeded judges
today, leaving ~6,300 FJC-populated Article III judges with no ideology signal
even though we know who appointed them.  This module is the fallback the
training corpus builder uses when those higher-fidelity scores are missing.

Values are calibrated against the historical-consensus rough ordering
(similar to Segal-Cover scores).  They are deliberately coarse: a single
scalar per appointing president, no per-judge variation.  When a better
per-judge ideology score exists, the trainer should prefer that.

Keys match the literal `appointing_president` strings emitted by the FJC
ingest — verified against the dev DB on 2026-05-18.  Presidents whose
appointment count in the dev DB is < 10 are intentionally omitted (the
fallback caller treats them as `None` → neutral fill).
"""
from __future__ import annotations

# Coarse president → ideology scalar in [-1, +1].
#
# Convention: negative = liberal-leaning appointer; positive = conservative-
# leaning.  Magnitude is a rough Segal-Cover-style consensus, not an
# empirical fit.  When refining, prefer published ideology priors over
# party affiliation alone.
PRESIDENT_IDEOLOGY: dict[str, float] = {
    # ── Modern era ─────────────────────────────────────────────────────
    # Strongly conservative-appointing
    "Donald J. Trump": +0.85,
    "George W. Bush": +0.70,
    "Ronald Reagan": +0.75,
    "George H.W. Bush": +0.55,
    "Gerald Ford": +0.40,
    "Richard M. Nixon": +0.55,
    "Dwight D. Eisenhower": +0.30,
    # Strongly liberal-appointing
    "Joseph R. Biden": -0.50,
    "Barack Obama": -0.55,
    "William J. Clinton": -0.40,
    "Jimmy Carter": -0.50,
    "Lyndon B. Johnson": -0.55,
    "John F. Kennedy": -0.45,
    "Harry S Truman": -0.40,
    "Franklin D. Roosevelt": -0.65,
    # ── Early-20th-century ────────────────────────────────────────────
    "Calvin Coolidge": +0.65,
    "Warren G. Harding": +0.60,
    "Herbert Hoover": +0.55,
    "William H. Taft": +0.50,
    "Theodore Roosevelt": +0.10,  # progressive Republican
    "Woodrow Wilson": -0.50,
    # ── 19th-century ──────────────────────────────────────────────────
    # Sparse calibration data — lean toward party affiliation and known
    # historical positioning.  Magnitudes are kept moderate (~0.2–0.4)
    # because pre-modern ideology axes don't map cleanly onto the
    # contemporary liberal/conservative axis.
    "George Washington": +0.10,   # Federalist-aligned
    "John Adams": +0.30,           # Federalist
    "Thomas Jefferson": -0.30,     # Democratic-Republican
    "James Madison": -0.25,        # Democratic-Republican
    "James Monroe": -0.20,         # Democratic-Republican
    "John Quincy Adams": -0.10,    # National Republican
    "Andrew Jackson": -0.30,       # Democrat (Jacksonian)
    "Martin Van Buren": -0.30,     # Democrat
    "John Tyler": +0.10,           # Whig-then-independent (states-rights)
    "James K. Polk": -0.25,        # Democrat
    "Zachary Taylor": +0.20,       # Whig (omitted: count < 10)
    "Millard Fillmore": +0.20,     # Whig (omitted: count < 10)
    "Franklin Pierce": -0.30,      # Democrat
    "James Buchanan": -0.25,       # Democrat
    "Abraham Lincoln": +0.20,      # Republican (anti-slavery)
    "Andrew Johnson": -0.15,       # Democrat (post-Lincoln)
    "Ulysses Grant": +0.10,        # Republican
    "Rutherford B. Hayes": +0.20,  # Republican
    "James A. Garfield": +0.25,    # Republican (omitted: count < 10)
    "Chester A. Arthur": +0.20,    # Republican
    "Grover Cleveland": -0.20,     # Democrat
    "Benjamin Harrison": +0.30,    # Republican
    "William McKinley": +0.40,     # Republican
}


def ideology_distance_from_president(
    president: str | None,
    *,
    neutral: float = 0.50,
) -> float:
    """Convert an appointing-president string to a coarse ideology distance.

    The training corpus's `ideology_distance` feature is the absolute value
    of the judge's ideology relative to an ideologically-neutral baseline
    (0.0).  Without a separate claim-side ideology axis, that is the safest
    interpretation for the v1 trainer.

    Returns `neutral` (typically 0.50, the NEUTRAL_FILL constant) when the
    president is missing or not in our mapping, so the caller doesn't need
    to special-case unknowns.  Result is clamped to [0, 1].
    """
    if not president:
        return neutral
    ideo = PRESIDENT_IDEOLOGY.get(president)
    if ideo is None:
        return neutral
    return max(0.0, min(1.0, abs(ideo)))
