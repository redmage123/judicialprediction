-- Sprint 16 / S16.3 — `attorneys` KG node + per-attorney win-rate rollup.
--
-- Sprint 15 retrain came in at Brier 0.2231 because 4 of 7 features were
-- pinned to NEUTRAL_FILL (0.5) in build_real_corpus.py — attorney_win_rate
-- was one of them. This table is the storage layer that lets the Layer-2
-- NLP extractor (rust/ingest-fetcher/src/extract.rs::run_extraction) roll up
-- per-attorney appearance/win counts so the trainer sees real signal.
--
-- Mirrors the `judges` table exactly: tenant-scoped, RLS-enabled, JSONB
-- `bio` carries the rollup (`bio.win_rate_proxy = { cases_analyzed,
-- wins_for_petitioner, win_rate }`). `win_rate` is the probability that
-- this attorney's client wins as petitioner — i.e. wins_for_petitioner /
-- cases_analyzed.
--
-- Idempotent: re-runs go through `ON CONFLICT (tenant_id,
-- normalized_name)` and `bio = bio || patch` so the recompute cron stays
-- clean.

BEGIN;

CREATE TABLE IF NOT EXISTS attorneys (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    normalized_name   text NOT NULL,
    full_name         text NOT NULL,
    bio               jsonb NOT NULL DEFAULT '{}',
    source            text,
    created_at        timestamp with time zone NOT NULL DEFAULT now(),
    updated_at        timestamp with time zone NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, normalized_name)
);

CREATE INDEX IF NOT EXISTS attorneys_normalized_name_idx
    ON attorneys (normalized_name);

CREATE TRIGGER trg_attorneys_updated_at
    BEFORE UPDATE ON attorneys
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

ALTER TABLE attorneys ENABLE ROW LEVEL SECURITY;
ALTER TABLE attorneys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON attorneys
    USING      (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

COMMENT ON TABLE attorneys IS
$tbl$
Sprint 16 / S16.3 — Layer-2 KG node for attorney name + per-attorney rollup.

`bio.win_rate_proxy` carries the aggregate the trainer reads:

    { "win_rate_proxy": {
        "cases_analyzed": 12,
        "wins_for_petitioner": 7,
        "win_rate": 0.5833333
    }}

Populated by rust/ingest-fetcher/src/extract.rs::run_extraction. Mirrors
the existing `judges.bio.severity_proxy` pattern; the extractor is
conservative (precision over recall) so a row only exists for attorneys
whose name was confidently parsed from an opinion's counsel block.
$tbl$;

GRANT SELECT, INSERT, UPDATE, DELETE ON attorneys TO jp_app;

COMMIT;
