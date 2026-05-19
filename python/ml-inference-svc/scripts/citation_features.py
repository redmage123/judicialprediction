"""
Citation density features — Sprint 20.4.

Extracts outgoing citations from each opinion's `full_text_plain` and
produces a small bag of features capturing how this opinion engages
with precedent. The original S20.4 plan called for petitioner-favorable
vs respondent-favorable citation counts — those require a
citation→opinion_id lookup that we don't have in-corpus yet. This
module ships the cheaper-but-real signal that doesn't need that lookup
and keeps the door open for the pet/resp counts in a future sprint.

Features per opinion:
  cite_total            — total outgoing citations
  cite_density          — citations per 1000 chars of text
  cite_scotus           — count of U.S. / S.Ct. / L.Ed. citations
  cite_circuit          — count of F.2d / F.3d / F.4th / F. citations
  cite_district         — count of F.Supp / F.R.D. citations
  cite_taxcourt         — count of T.C. citations
  cite_admin            — count of WL (Westlaw) + agency-decision citations
  cite_recent_count     — citations whose year metadata is within
                          15 years of this opinion's nominal year
                          (heuristic — citation year used if eyecite
                          parsed it, else 0)

Eyecite's warnings about Id. cross-references print to stderr; we
suppress them here because the volume is high and they don't affect
the feature values.

Cost: eyecite call is ~50-200ms per opinion on f3d-sized text.
~6,000 cases × ~100ms = ~10 min wall-clock for the v13 build.
"""
from __future__ import annotations

import os
import sys
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Iterable

import eyecite

# Reporter family classification. The keys are upper-cased reporter
# names eyecite reports in `groups["reporter"]`. Anything not matched
# here goes into 'cite_total' but no specific bucket — that's fine for
# the model because the buckets are additive features.
_SCOTUS_REPORTERS = {"U.S.", "S.CT.", "S. CT.", "L.ED.", "L.ED.2D"}
_CIRCUIT_REPORTERS = {"F.", "F.2D", "F.3D", "F.4TH"}
_DISTRICT_REPORTERS = {"F.SUPP.", "F.SUPP.2D", "F.SUPP.3D", "F.R.D."}
_TAXCOURT_REPORTERS = {"T.C.", "T.C.M.", "T.C.MEMO."}
_ADMIN_REPORTERS = {"WL", "LEXIS"}


@dataclass(frozen=True)
class CitationFeatures:
    cite_total: int
    cite_density: float
    cite_scotus: int
    cite_circuit: int
    cite_district: int
    cite_taxcourt: int
    cite_admin: int
    cite_recent_count: int


_ZERO = CitationFeatures(
    cite_total=0, cite_density=0.0, cite_scotus=0, cite_circuit=0,
    cite_district=0, cite_taxcourt=0, cite_admin=0, cite_recent_count=0,
)


@contextmanager
def _suppress_stderr():
    """Eyecite logs 'Unknown overlap case' to stderr; mute for batch runs."""
    saved = sys.stderr
    try:
        sys.stderr = open(os.devnull, "w")
        yield
    finally:
        sys.stderr.close()
        sys.stderr = saved


def _reporter_key(citation) -> str:
    """Normalize the reporter string from a FullCaseCitation."""
    groups = getattr(citation, "groups", None) or {}
    rep = groups.get("reporter")
    if rep:
        return rep.upper().strip()
    return ""


def _is_recent(citation, opinion_year: int | None, window_years: int = 15) -> bool:
    if opinion_year is None:
        return False
    md = getattr(citation, "metadata", None)
    year_raw = getattr(md, "year", None) if md is not None else None
    if not year_raw:
        return False
    try:
        cy = int(str(year_raw)[:4])
    except (ValueError, TypeError):
        return False
    return (opinion_year - cy) <= window_years and (opinion_year - cy) >= 0


def extract_citation_features(
    full_text_plain: str | None,
    opinion_year: int | None = None,
) -> CitationFeatures:
    """
    Return the citation-density feature bag for one opinion.

    `opinion_year` powers the recency feature only — when omitted, the
    `cite_recent_count` is always 0. Callers should pass the year
    parsed from `decided_at` (or the closest available proxy) when
    available.
    """
    if not full_text_plain:
        return _ZERO

    with _suppress_stderr():
        try:
            citations = list(eyecite.get_citations(full_text_plain))
        except Exception:
            # Eyecite occasionally panics on malformed/extreme inputs.
            # Returning ZERO keeps the parquet build resilient.
            return _ZERO

    total = 0
    scotus = circuit = district = tax = admin = recent = 0

    # Filter to FullCaseCitation only (skip IdCitation, SupraCitation,
    # etc., which are cross-references to citations already counted).
    for c in citations:
        ctype = type(c).__name__
        if ctype != "FullCaseCitation":
            continue
        total += 1
        rep = _reporter_key(c)
        if rep in _SCOTUS_REPORTERS:
            scotus += 1
        elif rep in _CIRCUIT_REPORTERS:
            circuit += 1
        elif rep in _DISTRICT_REPORTERS:
            district += 1
        elif rep in _TAXCOURT_REPORTERS:
            tax += 1
        elif rep in _ADMIN_REPORTERS:
            admin += 1
        if _is_recent(c, opinion_year):
            recent += 1

    density = (total / (len(full_text_plain) / 1000.0)) if full_text_plain else 0.0

    return CitationFeatures(
        cite_total=total,
        cite_density=density,
        cite_scotus=scotus,
        cite_circuit=circuit,
        cite_district=district,
        cite_taxcourt=tax,
        cite_admin=admin,
        cite_recent_count=recent,
    )
