"""Tests for the S6.3 regex extractor in scripts/enrich_layer3.py.

DB side is untested here — the regex pass is the only piece we can run
without a live Postgres + sample corpus.
"""
from __future__ import annotations

import importlib.util
import json
import os
import sys

import pytest

# Load the script under test as a module without putting `scripts/` on
# sys.path globally (the existing test layout keeps src/ on PYTHONPATH).
_HERE = os.path.dirname(os.path.abspath(__file__))
_SPEC = importlib.util.spec_from_file_location(
    "enrich_layer3",
    os.path.join(_HERE, "..", "scripts", "enrich_layer3.py"),
)
assert _SPEC and _SPEC.loader, "could not load enrich_layer3"
_MOD = importlib.util.module_from_spec(_SPEC)
sys.modules["enrich_layer3"] = _MOD
_SPEC.loader.exec_module(_MOD)  # type: ignore[union-attr]

extract_layer3 = _MOD.extract_layer3
serialise = _MOD.serialise


def test_writer_judge_extracted():
    text = "LAUBER, Judge: Petitioner appeals…"
    feat = extract_layer3(text)
    assert any(j.role == "writer" and j.name == "LAUBER" for j in feat.judges)


def test_concurring_judge_extracted():
    text = "...In a concurring opinion, KERRIGAN, J., wrote that the result is right but..."
    feat = extract_layer3(text)
    assert any(j.role == "concurring" and j.name == "KERRIGAN" for j in feat.judges)


def test_dissenting_judge_extracted():
    text = "...JONES, J., dissenting, would have held differently because..."
    feat = extract_layer3(text)
    assert any(j.role == "dissenting" and j.name == "JONES" for j in feat.judges)


def test_writer_role_takes_priority_over_concurring():
    text = (
        "LAUBER, Judge: ...\n"
        "In a concurring opinion, LAUBER, J., emphasised that..."
    )
    feat = extract_layer3(text)
    laubers = [j for j in feat.judges if j.name == "LAUBER"]
    assert len(laubers) == 1
    assert laubers[0].role == "writer"


def test_statutes_irc_form():
    text = "Petitioner seeks relief under I.R.C. § 6015(f) for tax year 2018."
    feat = extract_layer3(text)
    assert any("6015" in s for s in feat.statutes)


def test_statutes_usc_form():
    text = "The Court applies 26 U.S.C. § 7345 to the certification dispute."
    feat = extract_layer3(text)
    assert any("7345" in s for s in feat.statutes)


def test_statutes_deduplicated():
    text = "I.R.C. § 6662(a) applies; section 6662 also applies; I.R.C. § 6662 governs."
    feat = extract_layer3(text)
    # Different sub-section refs are distinct; the bare repeat dedups.
    matches = [s for s in feat.statutes if "6662" in s]
    # exactly two distinct strings: with and without `(a)`
    assert len(matches) == 2


def test_citation_with_reporter():
    text = "We follow Smith v. Commissioner, 142 T.C. 24 (2014)."
    feat = extract_layer3(text)
    assert any("Smith v. Commissioner" in c for c in feat.citations)


def test_element_summary_judgment():
    text = "Respondent moved for summary judgment on the deficiency."
    feat = extract_layer3(text)
    assert feat.elements["summary_judgment_motion"] is True


def test_element_section_6662():
    text = "Respondent also asserted a section 6662 accuracy-related penalty."
    feat = extract_layer3(text)
    assert feat.elements["section_6662_penalty"] is True


def test_element_reasonable_cause():
    text = "Petitioner argued that she had reasonable cause for the omission."
    feat = extract_layer3(text)
    assert feat.elements["reasonable_cause_defense"] is True


def test_element_absent_when_phrase_missing():
    text = "Respondent issued a notice of deficiency. Decision will be entered under Rule 155."
    feat = extract_layer3(text)
    assert feat.elements["reasonable_cause_defense"] is False
    assert feat.elements["willfulness_finding"] is False


def test_serialise_round_trip():
    text = "LAUBER, Judge: The section 6662 penalty applies."
    feat = extract_layer3(text)
    payload = json.loads(serialise(feat))
    assert payload["extractor_version"] == "regex-v1"
    assert payload["judges"][0]["name"] == "LAUBER"
    assert payload["judges"][0]["role"] == "writer"
    assert payload["elements"]["section_6662_penalty"] is True


def test_empty_text_returns_empty_features():
    feat = extract_layer3("")
    assert feat.judges == []
    assert feat.statutes == []
    assert feat.citations == []
    # Elements are always populated (all False on empty input).
    assert all(v is False for v in feat.elements.values())


def test_judge_regex_strips_section_header_prefix():
    """Regression: corpus opinions often have

        OPINION

              TORO, Judge: ...

    where the regex spans the blank line. The cleaner must reject the
    OPINION prefix and keep only the real name.
    """
    text = "OPINION\n\n      TORO, Judge: Petitioner appeals…"
    feat = extract_layer3(text)
    writer = [j for j in feat.judges if j.role == "writer"]
    assert len(writer) == 1
    assert writer[0].name == "TORO"


def test_citation_does_not_match_across_newlines():
    """Regression: 'Petitioners\\n\\nv.\\n\\nCOMMISSIONER' is the case caption,
    not a citation.  Multi-line v.-patterns are usually caption artefacts,
    not real precedent references.
    """
    text = "Petitioners\n\nv.\n\nCOMMISSIONER OF INTERNAL REVENUE"
    feat = extract_layer3(text)
    assert feat.citations == []
