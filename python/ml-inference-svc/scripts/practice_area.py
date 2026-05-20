"""
Substantive-law (`practice_area`) classifier — Sprint 22.1.

Mirrors `procedural_posture.py` in spirit: tier-1 regex over the first 4 KB
of opinion text, closed enum, conservative precedence. Statutes are the
strongest signal — a Title VII or §1983 reference is far more diagnostic
of subject matter than a generic word like "contract" floating in dicta.

Two-tier approach:
  * Tier 1 (regex over the first 4 KB of `full_text_plain`) covers the
    well-cited bodies of law. Cheap, deterministic; expected to label the
    majority of opinions on the first pass.
  * Tier 2 (LLM fallback) is wired separately in
    `scripts/extract_practice_area_llm.py` and only runs on `unknown`
    residuals via the local `claude -p` (no metered API spend).

The returned label is one of PRACTICE_AREAS so the trainer can one-hot the
column without ever seeing a surprise value.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

PRACTICE_AREAS: tuple[str, ...] = (
    "tax",                   # Title 26, Tax Court, deficiency proceedings
    "civil_rights",          # 42 U.S.C. §1983/§1981/§1985, Title VI/VII/IX, ADA
    "criminal",              # federal criminal code, habeas in criminal posture
    "employment",            # FLSA, ADEA, NLRA, ERISA-ish wage/labor
    "intellectual_property", # patent (35 U.S.C.), copyright (17), trademark / Lanham
    "bankruptcy",            # Title 11 / Bankruptcy Code
    "immigration",           # 8 U.S.C. / INA / asylum / removal / BIA-appeal language
    "antitrust",             # Sherman / Clayton Act, monopolization
    "securities",            # '33/'34 Act, SEC, 10b-5
    "administrative",        # APA review of agency action (non-immigration)
    "contract",              # contract / UCC / breach of contract
    "tort",                  # negligence / personal-injury / product liability
    "real_property",         # foreclosure, eminent domain, real estate
    "family",                # custody, divorce, child support
    "other",                 # genuinely none-of-the-above, NOT the same as unknown
    "unknown",               # tier-1 didn't match; tier-2 LLM may resolve
)


# Patterns ordered by precedence. Within each tuple: (label, compiled regex).
# More-specific statutory cites beat looser keyword matches; e.g. a "Title
# VII" hit fires civil_rights even if the opinion later mentions a contract.
_PATTERNS: tuple[tuple[str, re.Pattern], ...] = (
    # ── tax ────────────────────────────────────────────────────────────────
    (
        "tax",
        re.compile(
            r"\b(?:26\s+U\.?S\.?C\.?\b"
            r"|I\.?R\.?C\.?\s*§|Internal\s+Revenue\s+Code"
            r"|Tax\s+Court|notice\s+of\s+deficiency"
            r"|Commissioner\s+of\s+Internal\s+Revenue)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── civil rights ───────────────────────────────────────────────────────
    (
        "civil_rights",
        re.compile(
            r"\b(?:42\s+U\.?S\.?C\.?\s*§?\s*19(?:81|83|85|86|88)"
            r"|Title\s+(?:VI|VII|IX)\b"
            r"|Americans\s+with\s+Disabilities\s+Act\b|\bADA\s+(?:claim|case|action)"
            r"|Voting\s+Rights\s+Act|equal\s+protection\s+(?:clause|claim))",
            flags=re.IGNORECASE,
        ),
    ),
    # ── immigration ────────────────────────────────────────────────────────
    (
        "immigration",
        re.compile(
            r"\b(?:8\s+U\.?S\.?C\.?\b"
            r"|Immigration\s+and\s+Nationality\s+Act|\bINA\s+§"
            r"|Board\s+of\s+Immigration\s+Appeals|\bBIA\b"
            r"|asylum\s+(?:application|claim|petition)"
            r"|removal\s+(?:order|proceedings)|deportation\s+order"
            r"|withholding\s+of\s+removal)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── bankruptcy ─────────────────────────────────────────────────────────
    (
        "bankruptcy",
        re.compile(
            r"\b(?:11\s+U\.?S\.?C\.?\b|Bankruptcy\s+Code"
            r"|Chapter\s+(?:7|11|13)\s+(?:petition|debtor|case|filing)"
            r"|automatic\s+stay|discharge\s+(?:order|injunction)"
            r"|trustee\s+in\s+bankruptcy)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── intellectual property ──────────────────────────────────────────────
    (
        "intellectual_property",
        re.compile(
            r"\b(?:35\s+U\.?S\.?C\.?\b|patent\s+(?:infringement|claim\s+\d|application)"
            r"|17\s+U\.?S\.?C\.?\b|copyright\s+(?:infringement|registration|act)"
            r"|Lanham\s+Act|trademark\s+(?:infringement|registration|dilution)"
            r"|trade\s+secret\s+misappropriation)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── employment / labor ─────────────────────────────────────────────────
    (
        "employment",
        re.compile(
            r"\b(?:Fair\s+Labor\s+Standards\s+Act|\bFLSA\b"
            r"|Age\s+Discrimination\s+in\s+Employment\s+Act|\bADEA\b"
            r"|National\s+Labor\s+Relations\s+Act|\bNLRA\b|\bNLRB\b"
            r"|Family\s+(?:and\s+)?Medical\s+Leave\s+Act|\bFMLA\b"
            r"|\bERISA\b|29\s+U\.?S\.?C\.?\b"
            r"|wrongful\s+termination|hostile\s+work\s+environment)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── antitrust ──────────────────────────────────────────────────────────
    (
        "antitrust",
        re.compile(
            r"\b(?:Sherman\s+(?:Antitrust\s+)?Act|Clayton\s+Act"
            r"|15\s+U\.?S\.?C\.?\s*§?\s*[12]\b"
            r"|monopolization\s+claim|price[-\s]fixing|per\s+se\s+violation\b)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── securities ─────────────────────────────────────────────────────────
    (
        "securities",
        re.compile(
            r"\b(?:Securities\s+(?:Act|Exchange\s+Act)\s+of\s+19(?:33|34)"
            r"|Rule\s+10b-5|Section\s+10\(b\)"
            r"|Securities\s+and\s+Exchange\s+Commission|\bSEC\b"
            r"|insider\s+trading|investment\s+adviser\s+act)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── criminal ───────────────────────────────────────────────────────────
    (
        "criminal",
        re.compile(
            r"\b(?:18\s+U\.?S\.?C\.?\b"
            r"|sentencing\s+guidelines|sentenced\s+to\s+\d+\s+(?:months|years)"
            r"|indictment\s+charged|grand\s+jury|criminal\s+conviction"
            r"|guilty\s+plea|defendant\s+(?:pleaded|pled)\s+guilty"
            r"|firearms?\s+offense|drug\s+conspiracy)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── administrative (non-immigration / non-tax) ─────────────────────────
    (
        "administrative",
        re.compile(
            r"\b(?:Administrative\s+Procedure\s+Act|\bAPA\b"
            r"|5\s+U\.?S\.?C\.?\s*§?\s*70[6-9]"
            r"|arbitrary\s+and\s+capricious"
            r"|Chevron\s+(?:deference|step)"
            r"|notice[-\s]and[-\s]comment\s+rulemaking)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── contract ───────────────────────────────────────────────────────────
    (
        "contract",
        re.compile(
            r"\b(?:breach\s+of\s+contract|contract\s+claim"
            r"|Uniform\s+Commercial\s+Code|\bU\.?C\.?C\.?\s*§"
            r"|Statute\s+of\s+Frauds|specific\s+performance"
            r"|parol\s+evidence|meeting\s+of\s+(?:the\s+)?minds)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── tort ───────────────────────────────────────────────────────────────
    (
        "tort",
        re.compile(
            r"\b(?:negligence\s+claim|products?\s+liability"
            r"|personal\s+injury\s+(?:claim|action)|wrongful\s+death"
            r"|strict\s+liability|res\s+ipsa\s+loquitur"
            r"|duty\s+of\s+care|proximate\s+cause)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── real property ──────────────────────────────────────────────────────
    (
        "real_property",
        re.compile(
            r"\b(?:foreclosure\s+(?:action|sale|proceeding)"
            r"|eminent\s+domain|takings?\s+clause"
            r"|quiet\s+title|adverse\s+possession"
            r"|easement\s+(?:by|of)\b|deed\s+of\s+trust)",
            flags=re.IGNORECASE,
        ),
    ),
    # ── family ─────────────────────────────────────────────────────────────
    (
        "family",
        re.compile(
            r"\b(?:child\s+custody|divorce\s+(?:decree|action)"
            r"|child\s+support|spousal\s+(?:support|maintenance)"
            r"|paternity|adoption\s+proceeding|termination\s+of\s+parental\s+rights)",
            flags=re.IGNORECASE,
        ),
    ),
)


_HEAD_CHARS = 4000  # opinions often cite the controlling statute in the opening paragraphs


@dataclass(frozen=True)
class PracticeArea:
    label: str  # one of PRACTICE_AREAS
    tier: int   # 1 = regex match, 2 = LLM fallback, 0 = unknown


def classify_practice_area(full_text_plain: str | None) -> PracticeArea:
    """
    Tier-1 substantive-law classification. Returns PracticeArea(label, tier).
    tier=1 when a regex fired; tier=0 (label='unknown') when none did. The
    LLM fallback (`extract_practice_area_llm.py`) raises tier to 2.
    """
    if not full_text_plain:
        return PracticeArea(label="unknown", tier=0)
    head = full_text_plain[:_HEAD_CHARS]
    for label, pattern in _PATTERNS:
        if pattern.search(head):
            return PracticeArea(label=label, tier=1)
    return PracticeArea(label="unknown", tier=0)
