"""
Sprint 15 / S15.4 — FJC Biographical Directory ingest.

The Federal Judicial Center publishes a free CSV of every confirmed Article
III federal judge ("Judges of the United States Courts"). Sprint 14 surfaced
that most cafc opinions had ``judge_severity = NULL`` because no panel member
appeared in our small KG; this ingest backfills the KG so the extract-features
match rate climbs above ~14 %.

Pipeline:

    1. Download (or load) the FJC ``judges.csv`` (~3500 rows, 201 cols).
    2. Build a normalized name matching ``kg.rs::normalize_judge_name``
       (lowercase, strip ``hon. ``/``hon ``/``judge `` honorifics, collapse
       whitespace).  The Rust helper does *not* strip suffixes / punctuation;
       we mirror that exactly so an opinion-extracted "John Marshall Harlan"
       maps to the same key as the FJC row.
    3. Pick the **most recent** appointment (the CSV has up to six
       ``Court Type (N)`` / ``Confirmation Date (N)`` / ``Senior Status Date (N)``
       blocks); use it for ``appointing_president``, ``appointment_date``,
       ``senior_status_date``, ``confirmed_by_senate``.
    4. Upsert into ``judges`` keyed on ``(tenant_id, normalized_name)``:
        * INSERTs get ``source = 'fjc'``, ``source_id = nid``, the FJC bio
          payload nested under ``bio.fjc``.
        * UPDATEs **merge** the FJC payload under ``bio.fjc`` and refresh
          the four new biographic columns; existing ``bio.dime``,
          ``bio.mqs``, ``bio.jcs``, ``bio.severity_proxy`` keys are
          preserved by using ``bio || jsonb_build_object('fjc', …)``.

Re-runs are idempotent because the upsert key is the normalized name.

Usage:
    # Dry run, no DB writes:
    python scripts/fjc_ingest.py --dry-run

    # Real ingest from the live FJC URL:
    DATABASE_URL=postgresql://… python scripts/fjc_ingest.py

    # Or from a local cached CSV:
    python scripts/fjc_ingest.py --input /tmp/judges.csv
"""
from __future__ import annotations

import argparse
import csv
import io
import json
import os
import sys
import time
import urllib.request
from dataclasses import dataclass
from typing import Optional

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


FJC_URL = "https://www.fjc.gov/sites/default/files/history/judges.csv"
DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001"
DEFAULT_DB_URL = (
    "postgresql://judicialpredict:judicialpredict_dev_pwd"
    "@127.0.0.1:5454/judicialpredict_dev"
)

# Maximum number of "Court Type (N)" appointment blocks in the FJC CSV.
# As of 2026 the FJC schema goes up to 6; we read them all and pick the
# most recent.
MAX_APPT_BLOCKS = 6


# ── Name normalization ──────────────────────────────────────────────────────
#
# Mirror rust/ingest-fetcher/src/kg.rs::normalize_judge_name exactly:
#   * lowercase
#   * strip leading "hon. ", "hon ", "judge " (one pass each)
#   * collapse internal whitespace
#
# We deliberately do NOT strip suffixes ("Jr.", "Sr.", "III") or punctuation,
# because the Rust helper doesn't either.  A divergence here would produce
# normalized names that match in Python land but not in the opinion-side
# extraction.


def normalize_judge_name(raw: str) -> str:
    s = raw.lower()
    for prefix in ("hon. ", "hon ", "judge "):
        if s.startswith(prefix):
            s = s[len(prefix):]
    return " ".join(s.split())


# ── CSV parsing ─────────────────────────────────────────────────────────────


@dataclass
class FjcJudge:
    nid: str
    full_name: str
    normalized_name: str
    appointing_president: Optional[str]
    appointment_date: Optional[str]  # ISO date string or None
    senior_status_date: Optional[str]
    confirmed_by_senate: Optional[bool]
    bio_payload: dict


def _clean(value: Optional[str]) -> Optional[str]:
    if value is None:
        return None
    v = value.strip()
    return v if v else None


def _iso_date_or_none(value: Optional[str]) -> Optional[str]:
    """FJC dates are already ISO ``YYYY-MM-DD``; treat anything else as None."""
    v = _clean(value)
    if not v:
        return None
    # Defensive: only accept 10-character ISO dates so psycopg2 doesn't
    # choke on a malformed cell.
    if len(v) == 10 and v[4] == "-" and v[7] == "-":
        return v
    return None


def _pick_latest_appointment(row: dict) -> int:
    """Return the 1-based index of the appointment block with the most recent
    confirmation date (falling back to commission date, then to block 1).

    The CSV records up to ``MAX_APPT_BLOCKS`` appointments per judge in
    ``Court Type (N)`` columns; this picks the slot that best represents
    "current role".  We prefer the latest because cafc match-rate is dragged
    down by elderly judges whose first appointment was decades before they
    joined the Federal Circuit.
    """
    latest_idx = 1
    latest_key: tuple = ("", "")
    for n in range(1, MAX_APPT_BLOCKS + 1):
        court_type = _clean(row.get(f"Court Type ({n})"))
        if not court_type:
            continue
        # Sort key is (confirmation_date, commission_date) — both ISO strings
        # sort lexicographically and an empty string sorts first.
        conf = _iso_date_or_none(row.get(f"Confirmation Date ({n})")) or ""
        comm = _iso_date_or_none(row.get(f"Commission Date ({n})")) or ""
        key = (conf, comm)
        if key > latest_key:
            latest_key = key
            latest_idx = n
    return latest_idx


def parse_fjc_row(row: dict) -> Optional[FjcJudge]:
    nid = _clean(row.get("nid"))
    if not nid:
        return None

    first = _clean(row.get("First Name")) or ""
    middle = _clean(row.get("Middle Name")) or ""
    last = _clean(row.get("Last Name")) or ""
    suffix = _clean(row.get("Suffix")) or ""

    parts = [p for p in [first, middle, last] if p]
    if not parts:
        return None
    # Human-readable full name keeps the suffix ("Robert L. Smith Jr.")
    # because that's how opinions cite them.  The normalized_name does NOT
    # include the suffix — see normalize_judge_name and its Rust twin.
    full_name = " ".join(parts)
    if suffix:
        full_name = f"{full_name} {suffix}"

    normalized_name = normalize_judge_name(" ".join(parts))
    if not normalized_name:
        return None

    idx = _pick_latest_appointment(row)
    appointing_president = _clean(row.get(f"Appointing President ({idx})"))
    appointment_date = _iso_date_or_none(row.get(f"Confirmation Date ({idx})"))
    senior_status_date = _iso_date_or_none(row.get(f"Senior Status Date ({idx})"))

    # Senate Vote Type column is "Voice", "Roll Call", "Unknown", or empty.
    # "Voice" and "Roll Call" both mean confirmed; "Unknown" stays NULL.
    vote_type = _clean(row.get(f"Senate Vote Type ({idx})"))
    if vote_type in ("Voice", "Roll Call"):
        confirmed_by_senate: Optional[bool] = True
    elif vote_type == "Unknown" or vote_type is None:
        confirmed_by_senate = None
    else:
        confirmed_by_senate = None

    # bio.fjc payload — a compact subset; we don't dump all 201 cols.
    bio_payload = {
        "nid": nid,
        "court_type": _clean(row.get(f"Court Type ({idx})")),
        "court_name": _clean(row.get(f"Court Name ({idx})")),
        "appointment_title": _clean(row.get(f"Appointment Title ({idx})")),
        "appointing_president": appointing_president,
        "party_of_appointing_president":
            _clean(row.get(f"Party of Appointing President ({idx})")),
        "aba_rating": _clean(row.get(f"ABA Rating ({idx})")),
        "confirmation_date": appointment_date,
        "commission_date": _iso_date_or_none(row.get(f"Commission Date ({idx})")),
        "senior_status_date": senior_status_date,
        "termination": _clean(row.get(f"Termination ({idx})")),
        "termination_date": _iso_date_or_none(row.get(f"Termination Date ({idx})")),
        "birth_year": _clean(row.get("Birth Year")),
        "gender": _clean(row.get("Gender")),
        "race_or_ethnicity": _clean(row.get("Race or Ethnicity")),
        "appointment_block_index": idx,
    }
    # Drop None entries to keep the JSONB tidy.
    bio_payload = {k: v for k, v in bio_payload.items() if v is not None}

    return FjcJudge(
        nid=nid,
        full_name=full_name,
        normalized_name=normalized_name,
        appointing_president=appointing_president,
        appointment_date=appointment_date,
        senior_status_date=senior_status_date,
        confirmed_by_senate=confirmed_by_senate,
        bio_payload=bio_payload,
    )


def load_csv(path_or_url: str) -> list[FjcJudge]:
    """Load and parse the FJC CSV from a local path or HTTPS URL."""
    if path_or_url.startswith(("http://", "https://")):
        print(f"==> downloading {path_or_url}", file=sys.stderr)
        with urllib.request.urlopen(path_or_url, timeout=120) as resp:
            raw = resp.read()
        text = raw.decode("utf-8")
        reader = csv.DictReader(io.StringIO(text))
    else:
        print(f"==> reading {path_or_url}", file=sys.stderr)
        fh = open(path_or_url, encoding="utf-8")
        reader = csv.DictReader(fh)

    # Dedupe on normalized_name: Postgres' ON CONFLICT can't process two
    # rows targeting the same unique key in a single batch, so we collapse
    # duplicates here.  ~19 collisions in the FJC corpus (e.g. two distinct
    # "John Marshall Harlan"s, the elder and the grandson).  We keep the
    # *later* record because the FJC nid appears to grow roughly with judge
    # birth-year and the more recent judge is the one we're likelier to see
    # in modern opinion text.
    by_norm: dict[str, FjcJudge] = {}
    collisions = 0
    total_parsed = 0
    raw_records: list[tuple[str, FjcJudge]] = []  # (last_name_lower, judge)
    for row in reader:
        rec = parse_fjc_row(row)
        if rec is None:
            continue
        total_parsed += 1
        if rec.normalized_name in by_norm:
            collisions += 1
        by_norm[rec.normalized_name] = rec
        last_lower = (row.get("Last Name") or "").strip().lower()
        raw_records.append((last_lower, rec))

    out = list(by_norm.values())

    # ── Last-name aliases ───────────────────────────────────────────────────
    #
    # Opinion-side judge extraction (kg.rs::extract_judge_names) most often
    # captures a bare last-name token ("STOLL", "Holmes") and normalizes it
    # to a single lowercase word.  An FJC row keyed on `first middle last`
    # therefore never matches a single-token opinion extraction.
    #
    # To bridge the two key spaces we insert a *secondary* alias row keyed on
    # the last name alone whenever that last name is unique within the FJC
    # corpus (~81 % of FJC last names are unique).  Ambiguous last names
    # ("Smith" — 31 judges, "Jones" — 25) get NO alias; auto-aliasing one of
    # them would silently bind every "Smith" opinion to one specific judge.
    #
    # Alias rows share the same FJC bio payload and biographic columns, plus
    # a ``bio.fjc.alias_for_nid`` marker so we can audit them later.
    last_counts: dict[str, int] = {}
    for last_lower, _ in raw_records:
        if last_lower:
            last_counts[last_lower] = last_counts.get(last_lower, 0) + 1

    aliases_added = 0
    aliases_skipped_amb = 0
    aliases_skipped_collide = 0
    for last_lower, rec in raw_records:
        if not last_lower:
            continue
        if last_counts[last_lower] != 1:
            aliases_skipped_amb += 1
            continue
        # Skip if the last-name key is already the canonical normalized_name
        # for this judge (single-token names like "Cardozo").
        if last_lower == rec.normalized_name:
            continue
        # Skip if another FJC record already claims this last-name slot —
        # shouldn't happen given the uniqueness guard, but belt and braces.
        if last_lower in by_norm:
            aliases_skipped_collide += 1
            continue
        alias = FjcJudge(
            nid=rec.nid,
            full_name=rec.full_name,
            normalized_name=last_lower,
            appointing_president=rec.appointing_president,
            appointment_date=rec.appointment_date,
            senior_status_date=rec.senior_status_date,
            confirmed_by_senate=rec.confirmed_by_senate,
            bio_payload={**rec.bio_payload, "alias_for_nid": rec.nid},
        )
        by_norm[last_lower] = alias
        out.append(alias)
        aliases_added += 1

    print(
        f"==> parsed {total_parsed} FJC rows -> {len(out)} upsert candidates\n"
        f"     ({collisions} same-normalized-name collapses, "
        f"{aliases_added} last-name aliases added, "
        f"{aliases_skipped_amb} aliases skipped as ambiguous)",
        file=sys.stderr,
    )
    return out


# ── Database upsert ─────────────────────────────────────────────────────────

#
# Upsert SQL.
#
# Why this shape:
#   * ON CONFLICT on (tenant_id, normalized_name) — the unique constraint
#     `judges_tenant_id_normalized_name_key`.
#   * The bio merge uses `judges.bio || jsonb_build_object('fjc', EXCLUDED.bio->'fjc')`
#     so FJC's payload lands at `bio.fjc.*` without touching any sibling
#     keys (`dime`, `mqs`, `jcs`, `severity_proxy`).  This is the
#     protect-existing-data contract the Sprint 15 plan calls out.
#   * For the four new biographic columns we use COALESCE(EXCLUDED.x, judges.x)
#     so an FJC row with NULL appointment_date can't blank out a value
#     already set by an earlier run.
#
UPSERT_SQL = """
INSERT INTO judges (
    tenant_id,
    full_name,
    normalized_name,
    bio,
    source,
    source_id,
    appointing_president,
    appointment_date,
    senior_status_date,
    confirmed_by_senate
)
VALUES %s
ON CONFLICT (tenant_id, normalized_name) DO UPDATE SET
    -- Merge FJC payload under bio.fjc; sibling enrichments stay intact.
    bio = judges.bio || jsonb_build_object('fjc', EXCLUDED.bio->'fjc'),
    -- Only set source='fjc' if it wasn't already set by another ingest;
    -- existing DIME/MQ/JCS rows have source='courtlistener-test' or similar
    -- and we leave them be so provenance stays auditable.
    source = COALESCE(judges.source, EXCLUDED.source),
    source_id = COALESCE(judges.source_id, EXCLUDED.source_id),
    appointing_president = COALESCE(EXCLUDED.appointing_president, judges.appointing_president),
    appointment_date     = COALESCE(EXCLUDED.appointment_date, judges.appointment_date),
    senior_status_date   = COALESCE(EXCLUDED.senior_status_date, judges.senior_status_date),
    confirmed_by_senate  = COALESCE(EXCLUDED.confirmed_by_senate, judges.confirmed_by_senate),
    updated_at = now()
RETURNING (xmax = 0) AS inserted
"""


def upsert(conn, tenant_id: str, judges: list[FjcJudge], batch: int = 500) -> tuple[int, int]:
    """Bulk upsert judges, returning (inserted, updated) counts."""
    inserted_total = 0
    updated_total = 0

    with conn.cursor() as cur:
        # Set the RLS tenant for this session.
        cur.execute("SET LOCAL app.current_tenant_id = %s", (tenant_id,))

        # Build the value tuples.
        values = []
        for j in judges:
            bio_jsonb = json.dumps({"fjc": j.bio_payload})
            values.append((
                tenant_id,
                j.full_name,
                j.normalized_name,
                bio_jsonb,
                "fjc",
                j.nid,
                j.appointing_president,
                j.appointment_date,
                j.senior_status_date,
                j.confirmed_by_senate,
            ))

        for start in range(0, len(values), batch):
            chunk = values[start:start + batch]
            results = psycopg2.extras.execute_values(
                cur,
                UPSERT_SQL,
                chunk,
                template="(%s, %s, %s, %s::jsonb, %s, %s, %s, %s, %s, %s)",
                fetch=True,
            )
            for (was_inserted,) in results:
                if was_inserted:
                    inserted_total += 1
                else:
                    updated_total += 1
            print(
                f"    upserted batch {start // batch + 1} "
                f"({start + len(chunk)}/{len(values)})",
                file=sys.stderr,
            )

    return inserted_total, updated_total


# ── CLI ─────────────────────────────────────────────────────────────────────


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument(
        "--input",
        default=FJC_URL,
        help=f"FJC judges.csv path or URL (default: {FJC_URL})",
    )
    ap.add_argument(
        "--tenant-id",
        default=DEV_TENANT_ID,
        help=f"Tenant UUID (default: {DEV_TENANT_ID})",
    )
    ap.add_argument(
        "--database-url",
        default=os.environ.get("DATABASE_URL", DEFAULT_DB_URL),
        help="Postgres DSN; defaults to $DATABASE_URL or the dev DSN.",
    )
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse + summarize without DB writes.",
    )
    args = ap.parse_args()

    t0 = time.time()
    judges = load_csv(args.input)
    parse_secs = time.time() - t0

    # Summary stats.
    n_total = len(judges)
    n_with_appt_date = sum(1 for j in judges if j.appointment_date)
    n_with_president = sum(1 for j in judges if j.appointing_president)
    n_senior = sum(1 for j in judges if j.senior_status_date)
    n_confirmed = sum(1 for j in judges if j.confirmed_by_senate)
    print(
        f"\n==> parsed {n_total} judges in {parse_secs:.1f}s\n"
        f"     {n_with_appt_date} have an appointment date\n"
        f"     {n_with_president} have an appointing president\n"
        f"     {n_senior} have a senior-status date\n"
        f"     {n_confirmed} have confirmed_by_senate = true",
        file=sys.stderr,
    )

    if args.dry_run:
        # Show a sample row so the operator can eyeball it.
        if judges:
            sample = judges[0]
            print("\n==> sample row:", file=sys.stderr)
            print(
                f"     nid={sample.nid} full={sample.full_name!r}\n"
                f"     norm={sample.normalized_name!r}\n"
                f"     appt_date={sample.appointment_date} pres={sample.appointing_president!r}\n"
                f"     senior={sample.senior_status_date} confirmed={sample.confirmed_by_senate}\n"
                f"     bio_payload_keys={sorted(sample.bio_payload.keys())}",
                file=sys.stderr,
            )
        print("\n==> dry-run: no DB writes", file=sys.stderr)
        return 0

    print(f"\n==> connecting to {args.database_url.split('@')[-1]}", file=sys.stderr)
    conn = psycopg2.connect(args.database_url)
    try:
        t1 = time.time()
        inserted, updated = upsert(conn, args.tenant_id, judges)
        conn.commit()
        write_secs = time.time() - t1
        print(
            f"\n==> wrote {inserted} inserts, {updated} updates "
            f"in {write_secs:.1f}s",
            file=sys.stderr,
        )
    except Exception:
        conn.rollback()
        raise
    finally:
        conn.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
