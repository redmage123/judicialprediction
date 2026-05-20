# Sprint 22 Plan — substantive-law features + state-court corpus

## Goal

Two things, in that priority order:

1. **Substantive-law (`practice_area`) feature + corpus-wide citation graph.**
   The S21 champion (Brier 0.1768) discriminates well on procedural form (court,
   posture, text embedding) but is **subject-matter blind**: a contract appeal
   and a Title VII appeal in the Third Circuit look identical to the feature
   set. Labeling each opinion's substantive area (tax/contract/tort/civil-
   rights/IP/criminal/…) gives the model the dimension legal practitioners
   actually reason about. Pair this with the citation pet/resp-favored counts
   that S21.3 was forced to defer, and both gaps close together.

2. **Extend the corpus to New Jersey + California state courts.**
   The intake form already offers `nj-state` and `ca-state` as jurisdictions,
   but the v14/v17/v18 training corpora are **100% federal** (`f3d`/`us`/
   `cafc`/`bia`/`tax` — zero state rows). Any prediction on a state case is
   extrapolation with no support. Adding the bulk-dump state corpus closes
   that soundness gap and unlocks "real" state-jurisdiction predictions.

Sprint 21's headline observation drives Sprint 22's data plan: **bulk-dump
ingest is the only viable path** for state corpus and citation backfill.
The CourtListener REST quota is 125/day — re-fetching 5,937 opinions takes
≈48 days; doing tens of thousands of state opinions through REST is a
non-starter. The CL bulk-dump artifacts are the only sane source. Cheapest
items are scheduled first so each can land independently of the heavy ingest.

## Promotion gate

For the S22 retrain (after S22.1–S22.5 land):

- **Beat the current champion on Brier** (v18 if v18 wins this round; v17
  Brier 0.1768 otherwise) — that's the only signal-quality test that matters.
- **ECE ≤ 0.025** (small buffer over v17's 0.0181; do not regress past v17's
  by more than ~0.007). If recalibration is needed again, reuse the S21.5
  inference-form CV pattern.
- **Four-model spread ≤ 0.01** between the per-court base models (sanity
  check that no single base is dominating; matches the S20 gate).

Hard requirement: if practice-area labeling produces a category whose
held-out support is <50 cases, fold it into an `other` bucket before training
so a single small class can't drive the meta-blender.

## Tickets

### S22.1 — `practice_area` LLM tier-2 labeler (~4 h compute, ~0 cost)

Same pattern as the S21.2 posture labeler — local `claude -p` via the user's
auth, no metered API spend.

- Reuse `scripts/extract_posture_llm.py`'s shape; write
  `scripts/extract_practice_area_llm.py`.
- Closed enum: `tax`, `contract`, `tort`, `civil_rights`, `criminal`,
  `employment`, `intellectual_property`, `bankruptcy`, `immigration`,
  `administrative`, `family`, `securities`, `antitrust`, `real_property`,
  `other`.
- Source text from `case_documents.full_text_plain` (the same path that
  unblocked S21.2 to 4,350 rows in one pass).
- Tier-1 cheap regex first (a Title VII / NLRA / 26 U.S.C. / 42 U.S.C. § 1983
  / Sherman Act / Lanham Act pass), tier-2 LLM only for residual `unknown`.
  Aim ≥ 95% labeled.
- Cache to `data/practice_area_cache.json`, resumable.

Output: a column added to `real_corpus_v19.parquet` via
`scripts/assemble_corpus_v19.py` (extended from the v17 assembler).

### S22.2 — citation bulk-dump ingest (no REST quota)

The S21 finding stands: `case_document_citations` is empty and
`cites_json` is populated for only **61 / 5,937** v14 rows. Full backfill via
REST is blocked (125/day). CourtListener publishes a free citations bulk
dump (`citation-map`) — load that into `case_document_citations`.

- Extend `rust/ingest-fetcher` with a `load-citations-bulk` subcommand or
  add a one-shot Python loader (`scripts/load_citations_bulk.py`).
- Filter on import to keep only edges where BOTH endpoints already exist in
  our `case_documents` (otherwise we'd write FK-violating rows). Document
  the filter so future state ingest (S22.3) widens coverage automatically.
- Validation: target ≥ 90% of `f3d`/`us` v14 rows getting at least one
  outgoing citation populated.

### S22.3 — citation pet/resp-favored cited counts (was S21.3)

Now genuinely feasible. For each citing opinion, count the cited opinions
whose `outcome_for` ∈ {`petitioner`, `respondent`}:

- New numeric features: `cited_pet_favored`, `cited_resp_favored`,
  `cited_pet_ratio = cited_pet_favored / max(1, total_cited)`.
- **Temporal safety:** only count cited cases with `date_filed < citing
  date_filed`. The S21 plan flagged this; honor it explicitly so a 2010
  case can never be informed by a 2020 outcome.
- Add to corpus assembly. Train baseline run on the cited-counts-only delta
  to measure marginal lift before bundling with S22.4–S22.5.

### S22.4 — state-court bulk ingest (NJ + CA)

The big one. Extend `ingest-fetcher` to load CL bulk dumps for:

- California: `cal` (Supreme), `calctapp` (Courts of Appeal), `calag`
  (Attorney General opinions, if useful).
- New Jersey: `nj` (Supreme), `njsuperctappdiv` (Appellate Division),
  `njtaxct` (Tax Court).

Ingest pipeline mirrors the federal bulk path (`rust/ingest-fetcher/src/
fetch.rs`). Validation:

- Schema fits the existing `case_documents` (court_id text already supports
  arbitrary slugs).
- `outcome_for` is **expected to start at NULL** for state rows — S22.5
  populates it.
- Spot-check: 10 random opinions per court manually verified, looking for
  garbled text / wrong court mapping / encoding issues. Same drill that
  validated the original federal ingest.

### S22.5 — state-court outcome detection

`detect_outcome` in `rust/ingest-fetcher` was tuned for federal-circuit
phrasing ("affirmed", "reversed", "remanded", "petition denied"). NJ and CA
state appellate practice has its own conventions:

- NJ Appellate Division: "affirmed", "reversed", "modified", "remanded" —
  mostly aligned with federal, but party labels differ ("appellant" vs
  "petitioner" in some types of cases).
- CA Court of Appeal: "judgment is affirmed/reversed", "writ granted/denied",
  with the appellant/respondent split.

Plan:

- Tier-1 regex extension to `detect_outcome` covering the state conventions
  (additive — federal patterns stay first; new patterns guarded by court
  slug).
- Tier-2 LLM fallback on residuals via `claude -p` (same approach as S21.2
  posture and S22.1 practice area).
- Acceptance: ≥ 80% of state opinions resolved to {`petitioner`,
  `respondent`, `split`} before we even consider training on them.

### S22.6 — corpus v19 + retrain + promote (gated)

Bundle all four enrichments and produce a single new corpus:

- `data/real_corpus_v19.parquet`: v18 baseline + `practice_area` +
  `cited_pet_favored` / `cited_resp_favored` / `cited_pet_ratio` + the new
  NJ/CA state rows (so `jurisdiction` carries `California` / `New_Jersey`
  with real support, not just `Federal`).
- Re-embed only the NEW state-row text (the federal `legal_bert_emb.npy`
  sidecar carries through unchanged); extend `build_legal_embeddings.py` to
  resume from the existing sidecar by `_opinion_id` membership.
- Retrain via `train_first_models.py` (inference-form CV recalibration is
  already in place from S21.5).
- Promote iff the gate above passes. Backup the current champion first.

### S22.7 — web/UI gating + form copy

- Currently the intake form lets operators pick `ca-state` / `nj-state` but
  the model has no training support for them. Once S22.4 ships, that's no
  longer a soundness lie. Add an indicator of training support per
  jurisdiction (e.g., "n trained cases") so operators know whether a
  prediction has real coverage.
- Add `practice_area` to `PredictInput` (mirrors the S21.1 wiring of
  `opinion_text`/`court_id`). The intake form gets a `practice_area` select
  with the S22.1 enum.

### S22.8 — docs + model card

Update `MODEL_CARD.md` for the new champion, `docs/...` (if any) for the
expanded jurisdiction coverage, and the README "training data" section.

## Data sources, concretely

| sprint | source | mechanism | quota concern? |
|---|---|---|---|
| S22.1 practice_area | `case_documents.full_text_plain` (already local) | `claude -p` via local auth | none — local LLM auth |
| S22.2 citations | CL `citation-map` bulk dump | one-time download + load | no — bulk download, not REST |
| S22.3 cited counts | derived from S22.2 | SQL/Python | none |
| S22.4 NJ/CA opinions | CL bulk dumps per court slug | `ingest-fetcher` bulk path | no — bulk, not REST |
| S22.5 state outcomes | text patterns + `claude -p` fallback | same pattern as S22.1 | none |

The `COURTLISTENER_TOKEN` at `~/.config/judicialpredict/courtlistener.env`
is valid; the daily REST quota is 125 requests. **Do not** burn it on
backfill — it's reserved for the daily incremental ingest (`scripts/
courtlistener-daily.sh`, which already gates two courts per day inside
that quota).

## Risks & mitigations

- **State-court ingest produces low-quality outcome labels.**
  Mitigation: S22.5 acceptance threshold is 80% labeled-with-confidence
  before training. Anything below sits in a holdout pool and doesn't bias
  the model.
- **`practice_area` LLM disagrees with `case_type_hint` (S5.7).**
  Both are kept on each row; the trainer uses `practice_area`. Disagreement
  is logged so we can spot-check the LLM if a category looks anomalously
  high-win-rate.
- **State rows shift the base rate.**
  Federal v14 base rate is 0.284; California Court of Appeal historically
  affirms ≈ 80% of civil appeals. The per-court isotonic layer absorbs that
  — but `seed=42` train/test splits are now stratifying across a much wider
  jurisdiction mix, so check the per-court reliability table (the diagnostic
  we built in S21.5) before promoting.
- **Citation features leak future information.**
  Hard-blocked by the `date_filed` temporal check in S22.3. Audit the
  filter with a single SQL query before adding the column.

## What is intentionally out of scope

- **CourtListener REST backfill of anything.** Anything that needs more than
  125 GETs/day belongs in a bulk-dump ticket.
- **Federal Circuit-of-appeals expansion beyond the existing `f3d`/`cafc`.**
  District-court ingest (PACER) is a separate quagmire (paid, per-document
  fees) and not part of S22.
- **Model architecture changes.** Legal-BERT 768-dim stays; no swap to a
  bigger model this sprint. The S22 thesis is "richer features beats bigger
  model."

## Suggested order of operations

1. **S22.1** (practice_area labeler) — fully self-contained, validates the
   labeling pipeline end-to-end on a new dimension before we commit to the
   heavier ingest work.
2. **S22.2** (citation bulk load) — runs while S22.1 labels in the
   background; together they enable a v18.5 retrain that doesn't require
   any new ingest.
3. **(checkpoint)** baseline retrain with just S22.1 + S22.3 features added
   to v18 — measure lift; decide whether state ingest is the right next
   investment or whether posture + practice_area + citations exhaust the
   easy gains.
4. **S22.4 + S22.5** (NJ/CA ingest + outcomes) in parallel — same DB schema,
   different countries of pain.
5. **S22.6** retrain + promote on `real_corpus_v19.parquet`.
6. **S22.7 + S22.8** UI + docs as the visible-to-operator wrap-up.
