"""
Procedural posture extraction — Sprint 20.3.

Classifies each opinion into one of ~10 procedural-posture buckets
(motion to dismiss, summary judgment, cert petition, en banc rehearing,
etc.). The model's existing `procedural_motion_count` feature counts
generic motions; this one identifies *which* motion the case is about,
which is a much stronger predictor on real data (Daubert exclusions
have very different outcome distributions from cert petitions).

Two-tier approach:
  * Tier 1 (regex over the first 2K chars of full_text_plain) covers
    the canonical postures. Inexpensive, deterministic, ~70% expected
    coverage based on early-CAP / f3d sampling.
  * Tier 2 (LLM fallback) is wired separately in
    `scripts/extract_posture_llm.py` and only runs on cases where the
    regex returns 'unknown'. Off the hot training-build path.

The regex layer is conservative on precedence: more-specific patterns
are tried first. A case mentioning both "petition for certiorari" and
"motion to dismiss" gets classified as cert_petition (cert is the
top-level posture; the motion is internal to the cert proceeding).

The final value is one of POSTURE_CATEGORIES — a closed enum so the
trainer can one-hot encode without surprise values appearing.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

POSTURE_CATEGORIES: tuple[str, ...] = (
    "cert_petition",       # SCOTUS-specific top-level
    "en_banc",             # full-circuit rehearing (rare; high signal)
    "summary_judgment",    # Rule 56
    "motion_dismiss",      # Rule 12(b)(6) and friends
    "daubert",             # expert-evidence challenge
    "habeas",              # 2241 / 2254 / 2255
    "direct_appeal",       # appellate-only catch-all
    "agency_review",       # appellate review of admin decision (BIA, IRS)
    "rehearing",           # panel rehearing (non-en-banc)
    "unknown",             # tier-2 fallback bucket
)


# Patterns ordered by precedence — first match wins. Each pattern fires
# on the first 2K-char head of full_text_plain to keep regex cost low
# and to avoid body-text dicta false-matches.
_PATTERNS: tuple[tuple[str, re.Pattern], ...] = (
    (
        "cert_petition",
        re.compile(
            r"petition\s+for\s+(?:a\s+)?writ\s+of\s+certiorari"
            r"|writ\s+of\s+certiorari\s+(?:to|granted|denied)"
            r"|on\s+writ\s+of\s+certiorari"
            r"|certiorari\s+granted",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "en_banc",
        re.compile(
            r"\b(?:rehearing\s+en\s+banc|en\s+banc\s+(?:rehearing|review|consideration))\b",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "habeas",
        re.compile(
            r"\b(?:writ\s+of\s+habeas\s+corpus"
            r"|habeas\s+corpus\s+petition"
            r"|petition\s+for\s+(?:a\s+)?writ\s+of\s+habeas"
            r"|\b(?:28\s+U\.S\.C\.\s+)?(?:Section|§)?\s*2241\b"
            r"|\b(?:28\s+U\.S\.C\.\s+)?(?:Section|§)?\s*2254\b"
            r"|\b(?:28\s+U\.S\.C\.\s+)?(?:Section|§)?\s*2255\b)",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "daubert",
        re.compile(
            r"\bDaubert\s+(?:motion|hearing|challenge|standard|inquiry)\b"
            r"|motion\s+to\s+(?:exclude|preclude)\s+(?:expert|the\s+expert)",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "summary_judgment",
        re.compile(
            r"\b(?:motion\s+for\s+summary\s+judgment"
            r"|cross[-\s]motion\s+for\s+summary\s+judgment"
            r"|summary\s+judgment\s+(?:was\s+granted|was\s+denied)"
            r"|Rule\s+56\b)",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "motion_dismiss",
        re.compile(
            r"\b(?:motion\s+to\s+dismiss"
            r"|Rule\s+12\(b\)\(6\)"
            r"|Fed\.\s*R\.\s*Civ\.\s*P\.\s*12\(b\)\(6\)"
            r"|granted\s+the\s+motion\s+to\s+dismiss)\b",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "agency_review",
        re.compile(
            r"\b(?:petition\s+for\s+review\s+of\s+(?:a\s+)?(?:final\s+)?(?:order|decision|determination)"
            r"|appeal\s+from\s+(?:the\s+)?(?:Board\s+of\s+Immigration|Tax\s+Court|Commissioner)"
            r"|review\s+of\s+(?:the\s+)?(?:agency|administrative|Board)\b)",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "rehearing",
        re.compile(
            r"\b(?:petition\s+for\s+(?:panel\s+)?rehearing"
            r"|on\s+(?:petition\s+for\s+)?rehearing"
            r"|granted\s+rehearing)\b",
            flags=re.IGNORECASE,
        ),
    ),
    (
        "direct_appeal",
        re.compile(
            r"\b(?:appeal\s+from\s+the\s+(?:United\s+States\s+District\s+Court|district\s+court)"
            r"|appellant|appellee"
            r"|on\s+direct\s+appeal\b)",
            flags=re.IGNORECASE,
        ),
    ),
)


_HEAD_CHARS = 2000


@dataclass(frozen=True)
class Posture:
    label: str  # one of POSTURE_CATEGORIES
    tier: int  # 1 = regex match, 2 = LLM-fallback (today unused), 0 = unknown


def classify_procedural_posture(full_text_plain: str | None) -> Posture:
    """
    Tier-1 procedural posture classification. Returns Posture(label,
    tier). The label is always one of POSTURE_CATEGORIES; tier=1 when
    a regex fired, tier=0 when none did.
    """
    if not full_text_plain:
        return Posture(label="unknown", tier=0)
    head = full_text_plain[:_HEAD_CHARS]
    for label, pattern in _PATTERNS:
        if pattern.search(head):
            return Posture(label=label, tier=1)
    return Posture(label="unknown", tier=0)
