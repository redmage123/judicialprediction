"""
S6.3 — Layer-3 enrichment worker for `case_documents`.

Pulls rows where `layer3_extracted_at IS NULL`, runs the regex-based v1
extractor below, and writes the result back into `layer3_features` jsonb.
Idempotent; safe to re-run.

Current extractor: pure regex.  Captures meaningfully more than S5.7's
`classify_case_type` / `detect_outcome` pair — statutes, case citations,
multiple judges with roles (writer / concurring / dissenting), and a
short closed-set of binary procedural elements.

Sprint 7+ upgrade path (NOT in this commit):

  * spaCy `en_core_web_sm` + a tax-domain trained LoRA for legal NER.
  * `gemma-4-e4b-judicialpredict-en` LoRA fine-tuned on US Tax Court
    opinions.  The existing `gemma-4-e4b-eurlex-v1` LoRA on the RunPod is
    trained on Croatian EUR-LEX text and is the wrong corpus for US tax
    court English; using it here would produce noise.
  * BERTopic-style topic modelling.
  * Self-consistency / chain-of-thought verification of LLM outputs.

When that path arrives, the worker swaps `extract_layer3()` for an LLM
call and bumps the `extractor_version` string — DB shape is unchanged.

Usage:
    docker exec judicialpredict_ml_inference \\
        python scripts/enrich_layer3.py --database-url $DATABASE_URL [--force]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from dataclasses import asdict, dataclass, field
from typing import Iterable

try:
    import psycopg2
    import psycopg2.extras
except ImportError:
    print(
        "psycopg2 not installed; install it via `uv pip install psycopg2-binary` "
        "or run this script from a venv that has it.",
        file=sys.stderr,
    )
    raise

EXTRACTOR_VERSION = "regex-v1"

# ── Regex patterns ──────────────────────────────────────────────────────────
# Statute references — three common forms in US tax court opinions:
#   I.R.C. § 6662          (with periods)
#   IRC § 6662             (without)
#   26 U.S.C. § 6662       (chapter-cited)
# The lookahead avoids capturing trailing `.` from "§ 6662."
_STATUTE_RE = re.compile(
    r"\b(?:I\.?R\.?C\.?|26\s+U\.S\.C\.)\s*§\s*\d+[A-Z]?(?:\(\w+\))*",
    flags=re.IGNORECASE,
)

# Case citations — tax-court-flavoured.  Three patterns:
#   "Plaintiff v. Defendant"                  (no reporter)
#   "Plaintiff v. Defendant, 142 T.C. 24"     (T.C. reporter)
#   "Plaintiff v. Defendant, 999 F.3d 100"    (federal reporter)
# All quantifiers operate on `\s` (single whitespace) rather than `\s+` so
# the match can't cross a newline.  Newlines break party names visually
# in the corpus and the cross-line matches were almost always garbage
# (e.g. "Petitioners\n\nv.\n\nCOMMISSIONER").
_CITATION_RE = re.compile(
    r"[A-Z][A-Za-z'’\-]+(?: [A-Z][A-Za-z'’\-]+){0,3}"
    r" v\. "
    r"[A-Z][A-Za-z'’\-]+(?: [A-Z][A-Za-z'’\-]+){0,3}"
    r"(?:, \d{1,4} (?:T\.C\.|F\.(?:2d|3d)|U\.S\.|S\. Ct\.) \d+)?"
)

# Judges with role — tax-court flavour:
#   "LAUBER, Judge:"                       → role=writer
#   "LAUBER, J., delivered the opinion"    → role=writer
#   "concurring opinion by KERRIGAN, J."   → role=concurring
#   "JONES, J., dissenting"                → role=dissenting
#
# We constrain the name capture to non-whitespace and use a small known-bad
# header list (`OPINION`, `MEMORANDUM`, etc.) to avoid matching section
# headings that happen to precede a real judge name across a newline.
_JUDGE_NAME = r"[A-Z][A-Z'\-]{1,}(?:\s[A-Z][A-Z'\-]{1,}){0,2}"  # no \s+ across newlines
_JUDGE_WRITER_RE = re.compile(
    rf"\b({_JUDGE_NAME})"
    r"(?:,\s*(?:Judge|Chief\s+Judge)(?:[:\.])"
    r"|,\s*J\.,\s*delivered)"
)
_JUDGE_CONCURRING_RE = re.compile(
    rf"concurr(?:ing|ence)[^.]{{0,40}}\b({_JUDGE_NAME}),\s*J\."
)
_JUDGE_DISSENTING_RE = re.compile(
    rf"\b({_JUDGE_NAME}),\s*J\.,\s*dissenting"
)
# Words that look like all-caps single tokens but are tax-court section
# headings, not judges.  Filtered after the regex captures.
_HEADER_BLOCKLIST = frozenset({
    "OPINION", "MEMORANDUM", "ORDER", "FINDINGS", "BACKGROUND",
    "DISCUSSION", "CONCLUSION", "INTRODUCTION", "FACTS", "ANALYSIS",
    "HOLDING", "JUDGMENT", "DECISION", "SUMMARY", "DISSENT",
})

# Closed-set procedural elements — each detected by a small alternation.
# Boolean per element; the model can use these as binary features in the
# enriched feature path once Layer 3 is wired into createCase.
_ELEMENT_PATTERNS: dict[str, re.Pattern[str]] = {
    "summary_judgment_motion":     re.compile(r"\bsummary\s+judgment\b", re.IGNORECASE),
    "cross_motion":                re.compile(r"\bcross[-\s]?motion(?:s)?\b", re.IGNORECASE),
    "section_6662_penalty":        re.compile(r"section\s+6662\b|I\.R\.C\.\s*§\s*6662", re.IGNORECASE),
    "section_6663_fraud_penalty":  re.compile(r"section\s+6663\b|I\.R\.C\.\s*§\s*6663", re.IGNORECASE),
    "section_6651_failure_to_file": re.compile(r"section\s+6651\b|I\.R\.C\.\s*§\s*6651", re.IGNORECASE),
    "reasonable_cause_defense":    re.compile(r"reasonable\s+cause", re.IGNORECASE),
    "willfulness_finding":         re.compile(r"willful(?:ness|ly)?\b", re.IGNORECASE),
    "innocent_spouse_relief":      re.compile(r"innocent\s+spouse|section\s+6015", re.IGNORECASE),
    "collection_due_process":      re.compile(r"collection\s+due\s+process|section\s+6330", re.IGNORECASE),
    "tefra_partnership":           re.compile(r"\bTEFRA\b|partnership[-\s]level", re.IGNORECASE),
    "expert_witness":              re.compile(r"\bexpert\s+(?:witness|testimony)\b", re.IGNORECASE),
}


# ── Extractor ───────────────────────────────────────────────────────────────


@dataclass
class JudgeMention:
    name: str
    role: str  # "writer" | "concurring" | "dissenting"


@dataclass
class Layer3Features:
    extractor_version: str = EXTRACTOR_VERSION
    judges: list[JudgeMention] = field(default_factory=list)
    statutes: list[str] = field(default_factory=list)
    citations: list[str] = field(default_factory=list)
    elements: dict[str, bool] = field(default_factory=dict)


def _dedup_preserve_order(items: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for item in items:
        if item not in seen:
            seen.add(item)
            out.append(item)
    return out


def _clean_judge_name(name: str) -> str | None:
    """Normalise whitespace and reject anything that's just a section header.

    The judge regexes can match across blank-line gaps like
    `OPINION\\n\\n      TORO, Judge:` — first token `OPINION` is the
    section heading, not part of the judge name.  Strip leading
    blocklisted tokens; if nothing's left, reject.
    """
    name = " ".join(name.split())
    tokens = name.split(" ")
    while tokens and tokens[0] in _HEADER_BLOCKLIST:
        tokens.pop(0)
    if not tokens:
        return None
    return " ".join(tokens)


def extract_layer3(text: str) -> Layer3Features:
    """Pure regex pass — same input always produces the same output."""
    out = Layer3Features()

    # Judges — primary writer first, then concurring/dissenting.  The same
    # name may appear in both buckets (e.g. a panel judge who also wrote
    # a concurrence); the order of discovery preserves the writer label.
    seen_names: set[str] = set()
    for match in _JUDGE_WRITER_RE.finditer(text):
        name = _clean_judge_name(match.group(1))
        if name and name not in seen_names:
            out.judges.append(JudgeMention(name=name, role="writer"))
            seen_names.add(name)
    for match in _JUDGE_CONCURRING_RE.finditer(text):
        name = _clean_judge_name(match.group(1))
        if name and name not in seen_names:
            out.judges.append(JudgeMention(name=name, role="concurring"))
            seen_names.add(name)
    for match in _JUDGE_DISSENTING_RE.finditer(text):
        name = _clean_judge_name(match.group(1))
        if name and name not in seen_names:
            out.judges.append(JudgeMention(name=name, role="dissenting"))
            seen_names.add(name)

    out.statutes = _dedup_preserve_order(
        # Whitespace normalise so "I.R.C.   §  6662" and "I.R.C. § 6662" match.
        re.sub(r"\s+", " ", m).strip()
        for m in _STATUTE_RE.findall(text)
    )

    out.citations = _dedup_preserve_order(_CITATION_RE.findall(text))

    out.elements = {
        elem_name: bool(pattern.search(text))
        for elem_name, pattern in _ELEMENT_PATTERNS.items()
    }

    return out


def serialise(features: Layer3Features) -> str:
    """JSON the worker writes to the column.  Judges flatten via asdict."""
    payload = {
        "extractor_version": features.extractor_version,
        "judges": [asdict(j) for j in features.judges],
        "statutes": features.statutes,
        "citations": features.citations,
        "elements": features.elements,
    }
    return json.dumps(payload)


# ── DB-side worker ──────────────────────────────────────────────────────────


def run_worker(database_url: str, force: bool = False, limit: int | None = None) -> dict:
    """Pull rows, run the extractor, write back.  Returns per-run stats."""
    stats = {"scanned": 0, "updated": 0, "failed": 0, "elapsed_s": 0.0}
    started = time.time()

    where = "WHERE layer3_extracted_at IS NULL" if not force else ""
    limit_clause = f"LIMIT {int(limit)}" if limit else ""

    with psycopg2.connect(database_url) as conn:
        with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            cur.execute(
                f"SELECT id, full_text_plain FROM case_documents {where} "
                f"ORDER BY id {limit_clause}"
            )
            rows = cur.fetchall()
            stats["scanned"] = len(rows)

            for row in rows:
                try:
                    features = extract_layer3(row["full_text_plain"] or "")
                    payload = serialise(features)
                except Exception as e:  # noqa: BLE001 — log + continue
                    stats["failed"] += 1
                    print(f"[fail] doc_id={row['id']}: {e}", file=sys.stderr)
                    continue

                with conn.cursor() as update_cur:
                    update_cur.execute(
                        "UPDATE case_documents "
                        "SET layer3_features = %s::jsonb, "
                        "    layer3_extracted_at = now() "
                        "WHERE id = %s",
                        (payload, row["id"]),
                    )
                    stats["updated"] += 1
            conn.commit()

    stats["elapsed_s"] = round(time.time() - started, 2)
    return stats


# ── CLI ─────────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--database-url",
        default=os.environ.get("DATABASE_URL"),
        help="Postgres DSN; defaults to $DATABASE_URL.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Re-enrich rows even when layer3_extracted_at is already set.",
    )
    parser.add_argument("--limit", type=int, default=None)
    args = parser.parse_args()

    if not args.database_url:
        raise SystemExit("--database-url required (or set $DATABASE_URL)")

    stats = run_worker(args.database_url, force=args.force, limit=args.limit)
    print(
        f"enrich-layer3 scanned={stats['scanned']} "
        f"updated={stats['updated']} failed={stats['failed']} "
        f"elapsed_s={stats['elapsed_s']}"
    )


if __name__ == "__main__":
    main()
