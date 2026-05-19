"""
Party-type extraction — Sprint 20.2.

Returns the structural identity of the petitioner and respondent from
an opinion's `full_text_plain`, plus a pro-se flag for either side.
Coarse-grained on purpose — three categories per side ("corporation",
"government", "individual") capture nearly all of the outcome-related
variance without needing per-row review.

Outcome-rate priors that justify these features (rough estimates from
the legal-prediction literature):
  * pro-se petitioners win < 15% of the time at the federal-circuit
    level; the global base rate is ~28%. Huge signal.
  * government as respondent in tax / immigration / civil-rights
    appellate posture wins materially more often than as petitioner.
  * corporation-vs-individual splits the corpus into two regimes with
    different judge effects and different motion-grant rates.

Operates entirely on opinion text — no caption parsing or external KG.
The opinion's caption usually appears in the first 1,500 chars
("petitioner v. respondent" or "appellant v. appellee"), so we scope
the side-detection regex to that header window. The pro-se test scans
the full text because the relevant marker can appear in counsel blocks
that come later.

This module is the Sprint 20 phase-A path: pure Python, runs at parquet
build time. A phase-B follow-up moves it into the Rust extractor +
case_documents columns so inference-time featurization doesn't need to
re-parse the opinion, but that's only worth doing if the feature shows
real Brier lift.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

# ── Patterns ────────────────────────────────────────────────────────────────

# Government markers: U.S. federal + state + agency forms. Case-
# insensitive because CAP opinions caption parties in ALL CAPS just as
# often as title case ("UNITED STATES v. SMITH"). The caption window
# (1.5K head) is short enough that case-insensitive matching doesn't
# false-match body dicta.
_GOV_PATTERNS = re.compile(
    r"\b(?:"
    r"United\s+States(?:\s+of\s+America)?"
    r"|Commissioner(?:\s+of\s+Internal\s+Revenue|\s+of\s+Social\s+Security)?"
    r"|Secretary\s+of\s+(?:State|Defense|Labor|Treasury|the\s+Interior|the\s+Navy)"
    r"|State\s+of\s+\w+"
    r"|City\s+of\s+\w+"
    r"|County\s+of\s+\w+"
    r"|Attorney\s+General"
    r"|Department\s+of\s+\w+"
    r"|Internal\s+Revenue\s+Service|IRS"
    r"|Social\s+Security\s+Administration|SSA"
    r"|Federal\s+Trade\s+Commission|FTC"
    r"|Securities\s+and\s+Exchange\s+Commission|SEC"
    r"|National\s+Labor\s+Relations\s+Board|NLRB"
    r"|Federal\s+Communications\s+Commission|FCC"
    r"|Environmental\s+Protection\s+Agency|EPA"
    r")\b",
    flags=re.IGNORECASE,
)

# Corporation markers: legal entity suffixes that aren't ambiguous in
# real party names. Case-insensitive — "Inc." and "INC." both appear.
_CORP_PATTERNS = re.compile(
    r"\b(?:Inc|LLC|L\.L\.C|Corp|Corporation|Co\b\.?|Company|Ltd|"
    r"L\.P|LP\b|LLP|Holdings|Trust|Bank|N\.A|Association|"
    r"Partners(?:hip)?|Group|Industries|Enterprises|Technologies|Systems)\b\.?",
    flags=re.IGNORECASE,
)

# Pro-se markers: appear in counsel blocks or judicial introductions
# ("appearing pro se", "pro se petitioner", "Mr. Smith, pro se").
# Scanned over the full opinion (capped at first 8K chars).
_PRO_SE_PATTERNS = re.compile(
    r"\b(?:"
    r"pro\s+se"
    r"|representing\s+(?:himself|herself|themselves)"
    r"|appearing\s+pro\s+se"
    r"|pro\s+per"
    r")\b",
    flags=re.IGNORECASE,
)

# Caption-splitter — the "v." or "vs." between petitioner and respondent.
# Tolerates spacing/case quirks. Real captions also have "v.\n" (newline)
# so we allow whitespace including newlines.
_VS_SPLIT = re.compile(r"\s+v\.?(?:s\.?)?\s+", flags=re.IGNORECASE)


# Header window — opinion captions are reliably in the first ~1,500
# chars. Anything past that is body text where regexes start to misfire.
_CAPTION_HEAD_CHARS = 1500
# Pro-se scan window — counsel block can appear up to ~8K in.
_PRO_SE_HEAD_CHARS = 8000


CATEGORIES = ("individual", "corporation", "government")


@dataclass(frozen=True)
class PartyTypes:
    petitioner: str  # one of CATEGORIES
    respondent: str  # one of CATEGORIES
    pro_se: bool


def _classify_side(text: str) -> str:
    """
    Return one of CATEGORIES for a single side of the caption.
    Order of precedence: government > corporation > individual.
    """
    if _GOV_PATTERNS.search(text):
        return "government"
    if _CORP_PATTERNS.search(text):
        return "corporation"
    return "individual"


def classify_party_types(full_text_plain: str | None) -> PartyTypes:
    """
    Extract (petitioner_type, respondent_type, pro_se) from opinion
    text. Falls back to ("individual", "individual", False) when the
    caption is unparseable — those rows still get encoded, they just
    contribute no party-type signal.
    """
    if not full_text_plain:
        return PartyTypes("individual", "individual", False)

    head = full_text_plain[:_CAPTION_HEAD_CHARS]
    parts = _VS_SPLIT.split(head, maxsplit=1)
    if len(parts) == 2:
        pet_side, resp_side = parts
        # Trim each side to the chunk near the "v." marker so a long
        # paragraph in the petitioner's side doesn't sweep matches in
        # from unrelated body text.
        pet_side = pet_side[-400:]
        resp_side = resp_side[:400]
        pet = _classify_side(pet_side)
        resp = _classify_side(resp_side)
    else:
        # No "v." found — opinion may be a per-curiam disposition or
        # an early-CAP slip opinion whose caption is in a separate
        # syllabus. Fall through to defaults.
        pet = "individual"
        resp = "individual"

    pro_se_head = full_text_plain[:_PRO_SE_HEAD_CHARS]
    pro_se = bool(_PRO_SE_PATTERNS.search(pro_se_head))

    return PartyTypes(petitioner=pet, respondent=resp, pro_se=pro_se)
