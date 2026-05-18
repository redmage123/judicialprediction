-- Sprint 15 / S15.2 — schema for the real-corpus retrain.
--
-- Three changes that unblock the rest of Sprint 15:
--
--   1. case_outcome_labels — pre-coded labels from external sources
--      (SCDB for SCOTUS today; future learned classifier output too).
--      One row per (opinion_id, source); confidence lets a future
--      classifier score how sure it is.
--
--   2. case_documents.source — CHECK constraint listing the four
--      ingest sources we now support (courtlistener, cap, govinfo,
--      dawson). The column already exists with default 'courtlistener';
--      this just enforces the closed vocabulary.
--
--   3. judges — biographical columns from FJC's directory:
--      appointing_president, appointment_date, senior_status_date,
--      confirmed_by_senate. All nullable so existing DIME/MQ/JCS rows
--      stay valid; FJC ingest backfills.

BEGIN;

-- ── 1. case_outcome_labels ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS case_outcome_labels (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    opinion_id    bigint NOT NULL,
    source        text NOT NULL,
    outcome       text NOT NULL,
    confidence    real,
    ingested_at   timestamp with time zone NOT NULL DEFAULT now(),

    CONSTRAINT case_outcome_labels_source_chk
        CHECK (source IN ('scdb', 'detector', 'learned')),
    CONSTRAINT case_outcome_labels_outcome_chk
        CHECK (outcome IN ('petitioner', 'respondent', 'split')),
    CONSTRAINT case_outcome_labels_confidence_chk
        CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    CONSTRAINT case_outcome_labels_unique UNIQUE (opinion_id, source)
);

COMMENT ON TABLE case_outcome_labels IS
$tbl$
Sprint 15 — external / hand-coded outcome labels for case_documents.

The detector path (rust/ingest-fetcher/src/extract.rs::detect_outcome_for_court)
still writes directly to case_documents.outcome_for for backwards
compatibility. This table is the durable home for labels from external
ground-truth sources (SCDB for SCOTUS today) and any future learned
classifier output. When both detector-derived and SCDB labels exist
for the same opinion, the retrain pipeline prefers SCDB.
$tbl$;

CREATE INDEX IF NOT EXISTS case_outcome_labels_opinion_idx
    ON case_outcome_labels (opinion_id);
CREATE INDEX IF NOT EXISTS case_outcome_labels_source_idx
    ON case_outcome_labels (source);

-- ── 2. case_documents.source CHECK ────────────────────────────────────────

-- The column already exists with default 'courtlistener'. All existing
-- rows are 'courtlistener', so the CHECK can be added without a backfill
-- step. Listed alphabetically except courtlistener (the existing default).
ALTER TABLE case_documents
    DROP CONSTRAINT IF EXISTS case_documents_source_chk;
ALTER TABLE case_documents
    ADD CONSTRAINT case_documents_source_chk
    CHECK (source IN ('courtlistener', 'cap', 'dawson', 'govinfo'));

-- ── 3. judges biographical fields (FJC ingest fills these) ────────────────

ALTER TABLE judges
    ADD COLUMN IF NOT EXISTS appointing_president text,
    ADD COLUMN IF NOT EXISTS appointment_date     date,
    ADD COLUMN IF NOT EXISTS senior_status_date   date,
    ADD COLUMN IF NOT EXISTS confirmed_by_senate  boolean;

COMMENT ON COLUMN judges.appointing_president IS
    'Sprint 15 / S15.4 — appointing president per FJC Biographical Directory. NULL for judges not yet matched against FJC.';
COMMENT ON COLUMN judges.appointment_date IS
    'Sprint 15 / S15.4 — Senate-confirmation date per FJC. Used by the MQ resolver as an as-of upper bound.';

COMMIT;
