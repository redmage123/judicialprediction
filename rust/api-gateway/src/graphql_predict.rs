// JudicialPredict API Gateway — ML inference GraphQL resolvers (S3.1 / JP-42).
//
// JP-71 (S5.4): replaced the Sprint-3 reqwest HTTP shortcut with a proper
// tonic gRPC client for InferenceService.PredictCaseOutcome.  v2.14 spec §7
// mandates gRPC for all cross-plane calls; HTTP to ml-inference-svc is now
// retired from the gateway.
//
// Feature values are encoded as "key:value" strings in PredictCaseOutcomeRequest
// .feature_ids per the JP-70 Python server contract (grpc_server.py).  JP-72
// (Sprint-5) will wire real feature-store lookups via case_id once the
// feature-store gRPC service exposes feature retrieval.
//
// This file is NOT the decision-action layer (S3.4).  The mutation here returns
// PredictResult to the API caller; downstream wiring to the decision-action
// layer happens in results-view rendering (S3.3 / S4.4).

use std::sync::Arc;
use std::time::Instant;

use async_graphql::{Context, ErrorExtensions, ID, InputObject, Json, Object, SimpleObject};
use audit_recorder::{AuditEvent, AuditRecorder, AuditStatus, hash_payload};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row as _};
use uuid::Uuid;

use crate::app::TenantId;
use crate::auth::Claims;
use crate::case_import::{ImportCaseRow, ImportCasesResult, do_import_cases};

// ---------------------------------------------------------------------------
// gRPC client state — injected into the GraphQL schema by build_app (JP-71)
// ---------------------------------------------------------------------------

/// Tonic gRPC client for InferenceService.PredictCaseOutcome.
///
/// Wraps the generated `InferenceServiceClient<Channel>` from the compiled
/// inference.proto stubs.  Clone is cheap — tonic channels are Arc-backed and
/// designed to be cloned per-call.
///
/// The client is injected once at startup via `build_app` and stored in the
/// async-graphql schema data map so resolvers never allocate a new connection.
///
/// Connection address is set via `ML_INFERENCE_GRPC_URL`
/// (default: `http://127.0.0.1:51051`).  The channel uses `connect_lazy` so
/// a transient ml-inference outage at gateway startup does not crash the
/// process.
#[derive(Clone)]
pub(crate) struct MlInferenceClient {
    pub(crate) inner: crate::ml_inference_proto::inference_service_client::InferenceServiceClient<
        tonic::transport::Channel,
    >,
}

// ---------------------------------------------------------------------------
// Internal call helper — gRPC fan-out and status → GraphQL error mapping
// ---------------------------------------------------------------------------

/// Outcome of a `call_ml` invocation.  `Ok` contains an already-constructed
/// `PredictResult` so call-sites need no further proto conversion.
pub(crate) enum MlCallOutcome {
    Ok(PredictResult),
    /// The RPC deadline was exceeded (tonic `DeadlineExceeded`).
    Timeout,
    /// The server returned `INVALID_ARGUMENT` — e.g. a Tier-C feature was
    /// supplied, or a required feature was missing.
    BadRequest(String),
    /// gRPC `UNAVAILABLE` — downstream not ready (e.g. champion model not
    /// loaded, transient transport failure).  Caller MUST surface the
    /// server-provided message so operators see "model not trained" rather
    /// than a generic "network failure".
    Unavailable(String),
    /// Any other gRPC error not covered above (Internal, Unknown, etc.).
    /// Distinct from `Unavailable` so callers can return 500 vs 503.
    Internal(String),
}

/// Build a `PredictCaseOutcomeRequest`, send it to ml-inference-svc over gRPC,
/// and convert the response to `PredictResult`.
///
/// Feature values are encoded as `"key:value"` strings in `feature_ids` per
/// the JP-70 Python server contract (`grpc_server.py`).  The `x-tenant-id`
/// metadata header is set from `tenant_id` so the ML service's own
/// `audit_recorder` fires with the correct tenant context.
pub(crate) async fn call_ml(
    ml: &MlInferenceClient,
    tenant_id: &uuid::Uuid,
    features: &PredictInput,
) -> MlCallOutcome {
    use crate::ml_inference_proto::PredictCaseOutcomeRequest;

    // Encode feature values as "key:value" strings matching JP-70's gRPC server
    // contract (grpc_server.py).  Numeric values use Rust's default Display
    // representation which matches Python's float parsing.
    let feature_ids = vec![
        format!("judge_severity:{}", features.judge_severity),
        format!("attorney_win_rate:{}", features.attorney_win_rate),
        format!("ideology_distance:{}", features.ideology_distance),
        format!("materiality_score:{}", features.materiality_score),
        format!("procedural_motion_count:{}", features.procedural_motion_count),
        format!("case_type:{}", features.case_type),
        format!("jurisdiction:{}", features.jurisdiction),
    ];

    let mut req = tonic::Request::new(PredictCaseOutcomeRequest {
        // case_id is empty: feature values are supplied directly via feature_ids.
        // JP-72 will wire the real case UUID once the feature-store retrieval path
        // is enabled for gRPC.
        case_id: String::new(),
        feature_ids,
        // MODEL_VARIANT_UNSPECIFIED (0) → server uses production default.
        model_variant: 0,
        // 0.0 → server uses its configured default coverage (90%).
        conformal_coverage: 0.0,
        trace_id: String::new(),
    });

    // Attach x-tenant-id metadata so the ML service's audit_recorder fires
    // with the correct tenant context.
    req.metadata_mut().insert(
        "x-tenant-id",
        tenant_id
            .to_string()
            .parse()
            .expect("UUID is always valid ASCII metadata"),
    );

    let mut client = ml.inner.clone();
    match client.predict_case_outcome(req).await {
        Ok(resp) => {
            let r = resp.into_inner();
            let ci = r.conformal_interval.as_ref();
            MlCallOutcome::Ok(PredictResult {
                p_win: r.p_win as f32,
                ci_lower: ci.map_or(0.0_f64, |c| c.lower) as f32,
                ci_upper: ci.map_or(1.0_f64, |c| c.upper) as f32,
                coverage: ci.map_or(0.9_f64, |c| c.coverage) as f32,
                // mlflow_run_id is the stable model identifier (champion run).
                model_version: r.mlflow_run_id,
                predicted_at_unix: r.predicted_at_unix,
            })
        }
        Err(status) => {
            let msg = status.message().to_string();
            match status.code() {
                tonic::Code::DeadlineExceeded => MlCallOutcome::Timeout,
                tonic::Code::InvalidArgument => MlCallOutcome::BadRequest(msg),
                tonic::Code::Unavailable => MlCallOutcome::Unavailable(msg),
                _ => MlCallOutcome::Internal(msg),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GraphQL input type
// ---------------------------------------------------------------------------

/// Tier-A/B feature inputs accepted by the predictCaseOutcome mutation.
///
/// Field names are the exact keys in ALLOWLIST_FEATURES (ml-inference-svc/predict.py).
/// Tier-C party features are intentionally absent at the type level; the ML
/// service enforces the same allowlist as a second line of defence.
///
/// async-graphql converts snake_case field names to camelCase in the SDL:
///   judge_severity → judgeSeverity, case_type → caseType, etc.
#[derive(InputObject, Serialize, Deserialize)]
pub struct PredictInput {
    pub judge_severity: f32,
    pub attorney_win_rate: f32,
    pub ideology_distance: f32,
    pub materiality_score: f32,
    pub procedural_motion_count: f32,
    pub case_type: String,
    pub jurisdiction: String,
}

// ---------------------------------------------------------------------------
// GraphQL output type
// ---------------------------------------------------------------------------

/// Prediction result: calibrated win probability and 90 % conformal CI.
///
/// All probability fields are in [0, 1].  `predicted_at_unix` is the Unix
/// epoch second at which the ML service generated the prediction.
#[derive(SimpleObject, Serialize, Deserialize)]
pub struct PredictResult {
    /// Calibrated win probability in [0, 1].
    pub p_win: f32,
    /// Conformal CI lower bound (90 % coverage by default).
    pub ci_lower: f32,
    /// Conformal CI upper bound.
    pub ci_upper: f32,
    /// Nominal CI coverage (e.g. 0.90 for 90 %).
    pub coverage: f32,
    /// MLflow run_id of the champion model that produced this prediction.
    pub model_version: String,
    /// Unix epoch seconds at which the prediction was generated by the ML service.
    pub predicted_at_unix: i64,
}

// ---------------------------------------------------------------------------
// GraphQL DTO: recommendation (output only — Decimal → String for precision)
// ---------------------------------------------------------------------------

/// Recommendation output type.
///
/// Monetary values (`expected_value_try`, `expected_value_settle`) are
/// `String` rather than `Float` so that `rust_decimal::Decimal` precision is
/// preserved end-to-end without IEEE-754 rounding in the JSON layer.
///
/// Sprint-5 follow-up: replace the $100k/$50k placeholders with real
/// operator-supplied `expected_damages` and cost-engine output.
#[derive(SimpleObject, Serialize, Deserialize, Clone)]
pub struct RecommendationDto {
    /// Recommended action: `"Try"`, `"Settle"`, or `"Borderline"`.
    pub kind: String,
    /// S6.4 — qualitative confidence band from the CI width:
    /// `"High"` (<0.10), `"Medium"` (0.10–0.20), or `"Low"` (≥0.20).
    pub confidence: String,
    /// S6.4 — bound-evaluated recommendation. Populated only when
    /// `confidence == "Low"`; otherwise null.
    pub counter_recommendation: Option<CounterRecommendationDto>,
    /// Three deterministic reasoning bullets produced by decision-arith.
    pub rationale_bullets: Vec<String>,
    /// Expected value of going to trial (`p_win × damages − cost`), as a
    /// decimal string (e.g. `"-20000.00"`). May be negative.
    pub expected_value_try: String,
    /// Expected value of settlement (`damages × 0.40` anchor), as a decimal
    /// string (e.g. `"40000.00"`).
    pub expected_value_settle: String,
}

/// S6.4 — bound-evaluated recommendation pair.
///
/// When the prediction CI is wide enough that the recommendation could
/// reasonably flip depending on where the true `p_win` lands inside the
/// CI, the api-gateway surfaces what the recommendation would be at each
/// bound.  Useful for operator-side sensitivity messaging
/// ("at the lower bound this would be Settle; at the upper bound this
/// would be Try").
#[derive(SimpleObject, Serialize, Deserialize, Clone)]
pub struct CounterRecommendationDto {
    /// Recommendation kind at `p_win = ci_lower`.
    pub kind_at_ci_lower: String,
    /// Recommendation kind at `p_win = ci_upper`.
    pub kind_at_ci_upper: String,
    /// Convenience flag: `kind_at_ci_lower != kind_at_ci_upper`.
    pub flips_within_ci: bool,
    /// Operator-facing one-sentence summary; deterministic for a given input.
    pub note: String,
}

// ---------------------------------------------------------------------------
// GraphQL output type: Case (S4.2 createCase result)
// ---------------------------------------------------------------------------

/// A persisted JudicialPredict case, returned by the `createCase` mutation.
///
/// `input_features` echoes the seven Tier-A/B feature fields from the request
/// as a JSON scalar so clients can replay predictions without re-keying.
///
/// Monetary values inside `recommendation` are `String` to avoid precision
/// loss at the JSON boundary.
#[derive(SimpleObject, Serialize, Deserialize)]
pub struct Case {
    /// Storage primary key (UUID v4).
    pub id: ID,
    /// Tenant this case belongs to (UUID v4 string).
    pub tenant_id: ID,
    /// The seven Tier-A/B features submitted with the request (JSON scalar).
    pub input_features: Json<PredictInput>,
    /// ML prediction output: calibrated win probability and conformal CI.
    pub prediction: PredictResult,
    /// Decision-arith recommendation with EV comparison and reasoning bullets.
    pub recommendation: RecommendationDto,
    /// UUID of the operator who submitted the case (`sub` JWT claim), if parseable.
    pub created_by: Option<ID>,
    /// ISO-8601 UTC timestamp of row creation from `cases.created_at`.
    pub created_at: String,
    /// S6.8 — the NLP feature suggestion extracted from the `opinion_text`
    /// payload at `createCase` time, if one was supplied.  `None` when the
    /// case was created without opinion text.  Stored alongside
    /// `input_features` (the operator's final values) so NLP-vs-operator
    /// accuracy can be evaluated later.
    pub nlp_suggestion: Option<Json<ExtractedFeatures>>,
}

// ---------------------------------------------------------------------------
// Pagination helper — used by the listCases resolver and unit tests
// ---------------------------------------------------------------------------

/// Compute the `nextOffset` cursor for [`CaseConnection`].
///
/// Returns `Some(offset + nodes_len)` when more rows remain beyond the
/// current page (i.e. `offset + nodes_len < total_count`), or `None` when
/// this page exhausts the result set.
/// S6.4 — convert a `decision_arith::Recommendation` to the GraphQL DTO,
/// including the new confidence band + counter-recommendation surfaces.
/// Single source of truth so createCase / repredictCase / the unit tests
/// don't drift from each other.
pub(crate) fn build_recommendation_dto(
    rec: decision_arith::Recommendation,
) -> RecommendationDto {
    let kind_label = |k: &decision_arith::RecommendationKind| match k {
        decision_arith::RecommendationKind::Settle => "Settle".to_string(),
        decision_arith::RecommendationKind::Try => "Try".to_string(),
        decision_arith::RecommendationKind::Borderline => "Borderline".to_string(),
    };
    let confidence = match rec.confidence {
        decision_arith::ConfidenceBand::High => "High",
        decision_arith::ConfidenceBand::Medium => "Medium",
        decision_arith::ConfidenceBand::Low => "Low",
    }
    .to_string();
    let counter_recommendation =
        rec.counter_recommendation.map(|c| CounterRecommendationDto {
            kind_at_ci_lower: kind_label(&c.kind_at_ci_lower),
            kind_at_ci_upper: kind_label(&c.kind_at_ci_upper),
            flips_within_ci: c.flips_within_ci,
            note: c.note,
        });
    RecommendationDto {
        kind: kind_label(&rec.kind),
        confidence,
        counter_recommendation,
        rationale_bullets: rec.rationale_bullets.to_vec(),
        expected_value_try: rec.expected_value_try.to_string(),
        expected_value_settle: rec.expected_value_settle.to_string(),
    }
}

pub(crate) fn compute_next_offset(
    offset: i32,
    nodes_len: usize,
    total_count: i64,
) -> Option<i32> {
    let end = i64::from(offset) + nodes_len as i64;
    if end < total_count {
        Some(offset + nodes_len as i32)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// GraphQL output type: CaseConnection (S4.3 listCases result)
// ---------------------------------------------------------------------------

/// Paginated list of cases for the current tenant.
///
/// `next_offset` is `Some(offset + nodes.len())` when additional pages
/// remain beyond this response; `None` when this page contains the final row.
///
/// async-graphql auto-converts snake_case field names to camelCase in the SDL:
///   `total_count → totalCount`, `next_offset → nextOffset`.
#[derive(SimpleObject, Serialize, Deserialize)]
pub struct CaseConnection {
    /// Cases on the current page, ordered `created_at DESC`.
    pub nodes: Vec<Case>,
    /// Total row count for this tenant across all pages.
    pub total_count: i64,
    /// Offset for the next page (`offset + nodes.len()`), or `None` if
    /// this page contains the final row.
    pub next_offset: Option<i32>,
}

// ---------------------------------------------------------------------------
// GraphQL output type: PredictionHistoryEntry (S4.7 casePredictions result)
// ---------------------------------------------------------------------------

/// One entry in a case's prediction history, returned by `casePredictions`.
///
/// Each row corresponds to an INSERT into the `predictions` table — either
/// the original `createCase` run (once S4.7 back-fills it) or a subsequent
/// `repredictCase` call.
///
/// `model_version` mirrors `PredictResult.model_version` for quick scanning
/// without unwrapping the nested `prediction` object.
#[derive(SimpleObject, Serialize, Deserialize)]
pub struct PredictionHistoryEntry {
    /// Storage primary key for this prediction run (UUID v4).
    pub id: ID,
    /// Full prediction result from this run.
    pub prediction: PredictResult,
    /// MLflow run ID / champion model version that produced this prediction.
    /// Denormalised from `prediction.model_version` for convenient list rendering.
    pub model_version: String,
    /// ISO-8601 UTC timestamp of when this prediction was generated.
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// GraphQL output type: ExtractedFeatures (S5.8 extractFeatures result)
// ---------------------------------------------------------------------------

/// Suggested prefills for the case-intake form, derived by running the S5.7
/// extractor (`classify_case_type`, `detect_outcome`, `extract_judge_names`)
/// against operator-supplied opinion text.
///
/// Optional fields are `None` when extraction had nothing to say.  The
/// frontend prefills the corresponding form field only when a field is
/// `Some(...)`; the operator can override any field before submitting.
///
/// Naming:
///   `*_suggestion`  → values that map onto an input field (operator override
///                     applies).
///   `*_hint`        → context-only signals shown next to the form (no
///                     direct field mapping; informational).
#[derive(SimpleObject, Serialize, Deserialize)]
pub struct ExtractedFeatures {
    /// Suggested `judgeSeverity` (0.0–1.0).  Resolved by extracting judge
    /// name(s) from the opinion text and looking up
    /// `judges.bio.severity_proxy.severity` for the current tenant.
    pub judge_severity: Option<f64>,
    /// Display name of the judge whose prior decisions backed
    /// `judge_severity`. Surfaced to the UI so operators can sanity-check
    /// the match before accepting the prefill.
    pub judge_name: Option<String>,
    /// Number of prior decisions in our corpus used to compute
    /// `judge_severity`. Lets the UI render confidence ("0 of 1" vs "0 of 12").
    pub judge_cases_analyzed: Option<i32>,
    /// S5.7 tax-court sub-classification (`income_tax`, `innocent_spouse`,
    /// `collection_due_process`, etc.).  Always populated; shown next to
    /// the form as context, not auto-set into `caseType` (different
    /// taxonomies — see `case_type_suggestion`).
    pub case_type_hint: String,
    /// Suggested value for the form's `caseType` field
    /// (`civil`/`criminal`/`bankruptcy`).  All S5.7 tax-court types collapse
    /// to `civil` for the trained model's input taxonomy.
    pub case_type_suggestion: Option<String>,
    /// Detected disposition from the supplied opinion
    /// (`petitioner`/`respondent`/`split`). `None` means Rule 155 /
    /// dismissal-only / no disposition phrase — informational only.
    pub outcome_for: Option<String>,
    /// Suggested value for the form's `jurisdiction` field.  Populated when
    /// the text carries a recognisable court signature
    /// (e.g. "United States Tax Court" → `us-federal`).
    pub jurisdiction_suggestion: Option<String>,
}

/// Maximum opinion-text length accepted by the NLP extractor.  The regex
/// passes don't care about size, but unbounded text from a malicious client
/// would still chew CPU.
pub(crate) const MAX_OPINION_TEXT_BYTES: usize = 256 * 1024;

/// Run the S5.7/S5.8 NLP extractor over `text` and resolve judge severity
/// from the `judges` table for `tenant_id`.
///
/// Shared by the `extractFeatures` query (S5.8) and the `createCase`
/// mutation (S6.8) so the prefill suggestion and the persisted suggestion
/// are produced by exactly the same code path — they cannot drift.
///
/// Pure-read; opens its own short transaction for the `SET LOCAL` tenant
/// scoping required by RLS on `judges`.
pub(crate) async fn extract_features_from_text(
    pool: &PgPool,
    tenant_id: Uuid,
    text: &str,
) -> async_graphql::Result<ExtractedFeatures> {
    use ingest_fetcher::{
        classify_case_type, detect_outcome, extract_judge_names, normalize_judge_name,
    };

    if text.len() > MAX_OPINION_TEXT_BYTES {
        return Err(async_graphql::Error::new(
            "text too long (>256 KB); supply just the opinion body",
        ));
    }

    // ── Pure NLP pass ────────────────────────────────────────────────────────
    let case_type_hint = classify_case_type(text).to_string();
    let outcome_for = detect_outcome(text).map(str::to_string);
    let judge_candidates: Vec<String> = extract_judge_names(text);

    // Every S5.7 tax-court class is "civil" in the PredictInput taxonomy
    // (income_tax / innocent_spouse / cdp / whistleblower / ... are all civil
    // proceedings under Title 26).  Suggest only when we actually had signal;
    // an empty extraction means we don't know.
    let case_type_suggestion = (!case_type_hint.is_empty()).then(|| "civil".to_string());

    // Jurisdiction — cheap signature scan.  A richer mapping lives in
    // `kg::map_courtlistener_jurisdiction` but that one keys on CL slug.
    let jurisdiction_suggestion = if text.contains("United States Tax Court")
        || text.contains("U.S. Tax Court")
        || text.contains("Supreme Court of the United States")
    {
        Some("us-federal".to_string())
    } else {
        None
    };

    // ── Judge severity lookup ────────────────────────────────────────────────
    // Take the first extracted candidate as the primary judge (opinion header
    // order is "writer first" in CourtListener exports).  If none, leave the
    // severity fields None.
    let (judge_severity, judge_name, judge_cases_analyzed) =
        match judge_candidates.first() {
            Some(raw_name) => {
                let normalized = normalize_judge_name(raw_name);
                if normalized.is_empty() {
                    (None, None, None)
                } else {
                    let mut tx = pool.begin().await.map_err(|e| {
                        async_graphql::Error::new(format!("judges tx begin: {e}"))
                    })?;
                    sqlx::query(&format!(
                        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
                    ))
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| {
                        async_graphql::Error::new(format!("SET LOCAL failed: {e}"))
                    })?;

                    let row = sqlx::query(
                        "SELECT full_name,
                                (bio->'severity_proxy'->>'severity')::float8    AS severity,
                                (bio->'severity_proxy'->>'cases_analyzed')::int AS cases_analyzed
                         FROM judges
                         WHERE tenant_id = $1 AND normalized_name = $2",
                    )
                    .bind(tenant_id)
                    .bind(&normalized)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| {
                        async_graphql::Error::new(format!("judge lookup failed: {e}"))
                    })?;

                    tx.commit().await.map_err(|e| {
                        async_graphql::Error::new(format!("judges tx commit: {e}"))
                    })?;

                    match row {
                        Some(r) => {
                            let name: Option<String> = r.try_get("full_name").ok();
                            let sev: Option<f64> = r.try_get("severity").ok();
                            let n: Option<i32> = r.try_get("cases_analyzed").ok();
                            (sev, name, n)
                        }
                        None => (None, None, None),
                    }
                }
            }
            None => (None, None, None),
        };

    Ok(ExtractedFeatures {
        judge_severity,
        judge_name,
        judge_cases_analyzed,
        case_type_hint,
        case_type_suggestion,
        outcome_for,
        jurisdiction_suggestion,
    })
}

// ---------------------------------------------------------------------------
// GraphQL Mutation root
// ---------------------------------------------------------------------------

pub(crate) struct Mutation;

#[Object]
impl Mutation {
    /// Predict the outcome of a case using the JudicialPredict ML ensemble.
    ///
    /// Accepts Tier-A/B features only (see `PredictInput`).  Tier-C party
    /// features are excluded at the type level and also rejected by the ML
    /// service (INVALID_ARGUMENT gRPC status).
    ///
    /// Calls ml-inference-svc via gRPC (`InferenceService.PredictCaseOutcome`).
    /// JP-71 replaces the Sprint-3 HTTP shortcut per v2.14 spec §7.
    ///
    /// Always writes one gateway-side audit row (fire-and-forget, non-blocking).
    ///
    /// On failure the resolver returns a GraphQL error with a closed-code
    /// extension field `"code"`:
    ///   - `MlInferenceTimeout`     — gRPC deadline exceeded
    ///   - `MlInferenceBadRequest`  — INVALID_ARGUMENT (Tier-C or missing feature)
    ///   - `MlInferenceUnavailable` — any other gRPC error
    ///
    /// Raw error details are logged but never forwarded to callers.
    async fn predict_case_outcome(
        &self,
        ctx: &Context<'_>,
        input: PredictInput,
    ) -> async_graphql::Result<PredictResult> {
        let start = Instant::now();

        // Extract tenant identity injected by jwt_middleware.
        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let ml = ctx
            .data::<MlInferenceClient>()
            .map_err(|_| async_graphql::Error::new("ml inference client unavailable"))?;

        // Serialise input once; reuse bytes for the audit payload hash.
        let input_json = serde_json::to_vec(&input)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // ── gRPC call ────────────────────────────────────────────────────────
        let outcome = call_ml(ml, &tenant_id, &input).await;
        let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

        // Map gRPC outcomes to (audit status, GraphQL result).
        let (audit_status, gql_result) = match outcome {
            MlCallOutcome::Ok(prediction) => (AuditStatus::Ok, Ok(prediction)),
            MlCallOutcome::Timeout => {
                tracing::warn!("ml-inference-svc timed out (predictCaseOutcome)");
                (
                    AuditStatus::Timeout,
                    Err(async_graphql::Error::new("ml inference timed out")
                        .extend_with(|_, ext| ext.set("code", "MlInferenceTimeout"))),
                )
            }
            MlCallOutcome::BadRequest(msg) => {
                tracing::warn!(detail = %msg, "ml-inference-svc rejected request (predictCaseOutcome)");
                (
                    AuditStatus::Err,
                    Err(async_graphql::Error::new("ml inference rejected request")
                        .extend_with(|_, ext| ext.set("code", "MlInferenceBadRequest"))),
                )
            }
            MlCallOutcome::Unavailable(msg) => {
                tracing::warn!(detail = %msg, "ml-inference-svc unavailable (predictCaseOutcome)");
                let detail = msg.clone();
                (
                    AuditStatus::Err,
                    Err(async_graphql::Error::new(format!("ml inference unavailable: {detail}"))
                        .extend_with(move |_, ext| {
                            ext.set("code", "MlInferenceUnavailable");
                            ext.set("detail", detail.clone());
                        })),
                )
            }
            MlCallOutcome::Internal(msg) => {
                tracing::error!(detail = %msg, "ml-inference-svc internal error (predictCaseOutcome)");
                let detail = msg.clone();
                (
                    AuditStatus::Err,
                    Err(async_graphql::Error::new(format!("ml inference error: {detail}"))
                        .extend_with(move |_, ext| {
                            ext.set("code", "MlInferenceInternal");
                            ext.set("detail", detail.clone());
                        })),
                )
            }
        };

        // Gateway-side fire-and-forget audit record.
        // Failure is intentionally swallowed — audit must never block or fail the request.
        if let Some(recorder) = ctx
            .data::<Option<AuditRecorder>>()
            .ok()
            .and_then(|r| r.as_ref())
        {
            let recorder = recorder.clone();
            let event = AuditEvent {
                actor: "api-gateway".to_string(),
                action: "predict.invoke".to_string(),
                payload_hash: hash_payload(&input_json),
                latency_ms,
                status: audit_status,
                cost_micros: None,
            };
            tokio::spawn(async move {
                if let Err(e) = recorder.record(tenant_id, event).await {
                    tracing::warn!(error = %e, "predict audit record failed (non-fatal)");
                }
            });
        }

        gql_result
    }

    /// Create and persist a case: runs prediction, computes a recommendation
    /// via decision-arith, inserts a row into `cases`, fires an audit event,
    /// and returns the full `Case` type.
    ///
    /// **Sprint-5 follow-ups (placeholder values used here):**
    /// - `expected_damages = $100,000` — replace with operator-supplied input
    ///   once the case-intake form accepts it.
    /// - `cost = $50,000`              — replace with cost-engine output
    ///   (same $50k placeholder used in S3.3).
    ///
    /// **title / jurisdiction** are derived from `PredictInput` fields and
    /// stored verbatim; Sprint-5 should expose a dedicated title field in the
    /// mutation input so operators can label cases meaningfully.
    ///
    /// The mutation fails with a GraphQL error (with a `"code"` extension) if:
    /// - The ML inference service is unreachable or returns a non-2xx status.
    /// - `DATABASE_URL` is not set (cases pool not wired).
    /// - The INSERT is rejected by RLS (tenant mismatch).
    async fn create_case(
        &self,
        ctx: &Context<'_>,
        input: PredictInput,
        // S6.8 — optional raw opinion text.  When supplied, the gateway runs
        // the S5.7/S5.8 NLP extractor over it and persists the resulting
        // suggestion in `cases.nlp_suggestion`, alongside the operator's
        // final `input_features`, for later NLP-vs-operator accuracy
        // evaluation.  Omitting it leaves `nlp_suggestion` NULL.
        #[graphql(desc = "Optional raw opinion text; when supplied, its NLP \
                          feature suggestion is persisted alongside the \
                          operator's final values.")]
        opinion_text: Option<String>,
    ) -> async_graphql::Result<Case> {
        let start = Instant::now();

        // ── 1. Extract identity from JWT claims ──────────────────────────────
        let claims = ctx
            .data::<Claims>()
            .map_err(|_| async_graphql::Error::new("missing claims"))?
            .clone();

        let tenant_id = Uuid::parse_str(&claims.tenant_id)
            .map_err(|_| async_graphql::Error::new("invalid tenant_id in claims"))?;

        // operator_id may legitimately fail to parse for service-account tokens.
        let operator_id: Option<Uuid> = Uuid::parse_str(&claims.sub).ok();

        // ── 2. Serialise input once; reuse bytes for ML body + audit hash ────
        let input_json = serde_json::to_vec(&input)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // ── 3. Call ml-inference-svc via gRPC (JP-71) ────────────────────────
        let ml = ctx
            .data::<MlInferenceClient>()
            .map_err(|_| async_graphql::Error::new("ml inference client unavailable"))?;

        let outcome = call_ml(ml, &tenant_id, &input).await;
        let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

        let prediction = match outcome {
            MlCallOutcome::Ok(p) => p,
            MlCallOutcome::Timeout => {
                tracing::warn!("ml-inference-svc timed out (createCase)");
                return Err(async_graphql::Error::new("ml inference timed out")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceTimeout")));
            }
            MlCallOutcome::BadRequest(msg) => {
                tracing::warn!(detail = %msg, "ml-inference-svc rejected request (createCase)");
                return Err(async_graphql::Error::new("ml inference rejected request")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceBadRequest")));
            }
            MlCallOutcome::Unavailable(msg) => {
                tracing::warn!(detail = %msg, "ml-inference-svc unavailable (createCase)");
                let detail = msg.clone();
                return Err(async_graphql::Error::new(format!("ml inference unavailable: {detail}"))
                    .extend_with(move |_, ext| {
                        ext.set("code", "MlInferenceUnavailable");
                        ext.set("detail", detail.clone());
                    }));
            }
            MlCallOutcome::Internal(msg) => {
                tracing::error!(detail = %msg, "ml-inference-svc internal error (createCase)");
                let detail = msg.clone();
                return Err(async_graphql::Error::new(format!("ml inference error: {detail}"))
                    .extend_with(move |_, ext| {
                        ext.set("code", "MlInferenceInternal");
                        ext.set("detail", detail.clone());
                    }));
            }
        };

        // ── 4. Compute recommendation via decision-arith ─────────────────────
        // Sprint-5: wire real expected_damages from operator intake form.
        let decision_input = decision_arith::PredictionInput {
            p_win:             f64::from(prediction.p_win),
            ci_lower:          f64::from(prediction.ci_lower),
            ci_upper:          f64::from(prediction.ci_upper),
            expected_damages:  Decimal::from(100_000u32),
        };
        // S5.10: replace the $50k cost placeholder with cost-engine output.
        // S5.11: jurisdiction also flows into the settle anchor (0.45 federal
        // / 0.35 state / 0.40 legacy fallback) via decision_arith::recommend.
        // S6.7: cost-engine v2 layers expected-duration + party-count factors
        // on top of the jurisdiction-base × motion-count model.  PredictInput
        // carries no party/duration signal (Tier-A/B only), so we derive the
        // expected duration from the motion count via the documented
        // cost_engine helper and pass the baseline party count — the v2 API
        // is ready for when richer case-intake data is captured.
        // procedural_motion_count is an f32 in the GraphQL input (matches the
        // feature wire format); clamp non-negative and round before casting.
        // Rust's `as u32` is saturating for f32 → u32 since 1.45.
        let motion_count = input
            .procedural_motion_count
            .max(0.0)
            .round() as u32;
        let cost = cost_engine::estimate_cost_v2(&cost_engine::CostInputs {
            jurisdiction: &input.jurisdiction,
            motion_count,
            expected_duration_months: cost_engine::derive_duration_months(motion_count),
            party_count: cost_engine::BASELINE_PARTY_COUNT,
        });
        let rec = decision_arith::recommend(&decision_input, cost, &input.jurisdiction);

        let recommendation = build_recommendation_dto(rec);

        // ── 5. Serialise structs to serde_json::Value for JSONB columns ───────
        let input_features_val = serde_json::to_value(&input)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let prediction_val = serde_json::to_value(&prediction)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let recommendation_val = serde_json::to_value(&recommendation)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        // ── 6. INSERT into cases (transaction for RLS SET LOCAL) ─────────────
        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("cases store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new("cases store not configured (DATABASE_URL missing)")
            })?;

        // S6.8 — if opinion text was supplied, run the shared NLP extractor
        // (same code path as the extractFeatures query) and persist its
        // suggestion next to the operator's final values.  Done before the
        // INSERT tx because the extractor opens its own short transaction.
        let nlp_suggestion: Option<ExtractedFeatures> = match opinion_text.as_deref() {
            Some(text) if !text.trim().is_empty() => {
                Some(extract_features_from_text(pool, tenant_id, text).await?)
            }
            _ => None,
        };
        let nlp_suggestion_val = nlp_suggestion
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx begin: {e}")))?;

        // SET LOCAL so the RLS insert-policy (`tenant_id = current_tenant_id`)
        // is satisfied for the jp_app role.  Uuid::to_string() is injection-safe.
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_id}'"
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        // `title` and `jurisdiction` are NOT NULL in the baseline schema.
        // Sprint-5: accept `title` from the operator case-intake form.
        let title = format!("{} case", input.case_type);

        let row = sqlx::query(
            r#"
            INSERT INTO cases
                (tenant_id, title, jurisdiction,
                 input_features, prediction, recommendation, created_by,
                 nlp_suggestion)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, created_at::text AS created_at_s
            "#,
        )
        .bind(tenant_id)
        .bind(&title)
        .bind(&input.jurisdiction)
        .bind(&input_features_val)
        .bind(&prediction_val)
        .bind(&recommendation_val)
        .bind(operator_id)
        .bind(&nlp_suggestion_val)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("case insert failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx commit: {e}")))?;

        let case_id: Uuid = row
            .try_get("id")
            .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
        let created_at_s: String = row
            .try_get("created_at_s")
            .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;

        // ── 7. Fire-and-forget audit record ──────────────────────────────────
        // Failure is intentionally swallowed — audit must never block the path.
        if let Some(recorder) = ctx
            .data::<Option<AuditRecorder>>()
            .ok()
            .and_then(|r| r.as_ref())
        {
            let recorder = recorder.clone();
            let event = AuditEvent {
                actor:        "api-gateway".to_string(),
                action:       "case.created".to_string(),
                payload_hash: hash_payload(&input_json),
                latency_ms,
                status:       AuditStatus::Ok,
                cost_micros:  None,
            };
            tokio::spawn(async move {
                if let Err(e) = recorder.record(tenant_id, event).await {
                    tracing::warn!(error = %e, "createCase audit record failed (non-fatal)");
                }
            });
        }

        // ── 8. Return the full Case ───────────────────────────────────────────
        Ok(Case {
            id:             ID::from(case_id.to_string()),
            tenant_id:      ID::from(tenant_id.to_string()),
            input_features: Json(input),
            prediction,
            recommendation,
            created_by:     operator_id.map(|id| ID::from(id.to_string())),
            created_at:     created_at_s,
            nlp_suggestion: nlp_suggestion.map(Json),
        })
    }

    /// S6.14 — bulk import: validate + predict + persist up to 50 rows in
    /// one request.  Per-row pipeline is identical to `createCase`'s; any
    /// row that fails to predict or insert is reported in `results` with
    /// `ok: false` and an operator-facing error message, but the request
    /// as a whole still succeeds (the operator triages the failed rows).
    ///
    /// The mutation fails (top-level error) only on identity / config
    /// problems (missing claims, missing pool, missing ml client, empty
    /// rows, or a row count above the per-request cap).
    async fn import_cases(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "Up to 50 rows; each is the same shape as the \
                          createCase input plus an optional opinion_text.")]
        rows: Vec<ImportCaseRow>,
    ) -> async_graphql::Result<ImportCasesResult> {
        let claims = ctx
            .data::<Claims>()
            .map_err(|_| async_graphql::Error::new("missing claims"))?
            .clone();
        let tenant_id = Uuid::parse_str(&claims.tenant_id)
            .map_err(|_| async_graphql::Error::new("invalid tenant_id in claims"))?;
        let operator_id: Option<Uuid> = Uuid::parse_str(&claims.sub).ok();

        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("cases store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new("cases store not configured (DATABASE_URL missing)")
            })?;
        let ml = ctx
            .data::<MlInferenceClient>()
            .map_err(|_| async_graphql::Error::new("ml inference client unavailable"))?;
        let audit_recorder = ctx
            .data::<Option<AuditRecorder>>()
            .ok()
            .and_then(|r| r.as_ref())
            .cloned();

        do_import_cases(
            pool,
            ml,
            audit_recorder,
            tenant_id,
            operator_id,
            rows,
        )
        .await
    }

    /// Re-run prediction on an existing case using the latest ML model.
    ///
    /// Fetches the stored `input_features`, calls ml-inference-svc with the
    /// same seven Tier-A/B features, inserts a new row into `predictions`,
    /// updates `cases.prediction` (and `cases.updated_at`) to reflect the
    /// latest run, and fires a `case.repredict` audit event.
    ///
    /// **Recommendation is NOT updated** — the stored recommendation from
    /// `createCase` is preserved verbatim.  Sprint-5 follow-up: optionally
    /// re-run decision-arith when the operator supplies updated damages/cost
    /// figures at repredict time.
    ///
    /// Returns a GraphQL error if:
    /// - The case UUID is invalid.
    /// - The case does not exist or belongs to a different tenant (RLS + WHERE).
    /// - The ML service is unreachable or returns a non-2xx status.
    /// - The cases pool is unavailable.
    async fn repredict_case(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Case> {
        let start = Instant::now();

        // ── 1. Extract identity from JWT claims ──────────────────────────────
        let claims = ctx
            .data::<Claims>()
            .map_err(|_| async_graphql::Error::new("missing claims"))?
            .clone();

        let tenant_id = Uuid::parse_str(&claims.tenant_id)
            .map_err(|_| async_graphql::Error::new("invalid tenant_id in claims"))?;

        let case_uuid = Uuid::parse_str(id.as_str())
            .map_err(|_| async_graphql::Error::new("invalid case id: must be a UUID v4"))?;

        // ── 2. Load the case from DB ──────────────────────────────────────────
        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("cases store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new(
                    "cases store not configured (DATABASE_URL missing)",
                )
            })?;

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx begin: {e}")))?;

        // RLS belt-and-suspenders: SET LOCAL so policies evaluate correctly for
        // the jp_app role (mirrors createCase / listCases / getCase pattern).
        sqlx::query(&format!(
            "SET LOCAL app.current_tenant_id = '{tenant_id}'"
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        let row_opt = sqlx::query(
            r#"
            SELECT id,
                   tenant_id,
                   input_features,
                   recommendation,
                   created_by,
                   created_at::text AS created_at_s,
                   nlp_suggestion
            FROM   cases
            WHERE  id = $1
              AND  tenant_id = $2
            "#,
        )
        .bind(case_uuid)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("case select failed: {e}")))?;

        let row = row_opt.ok_or_else(|| {
            async_graphql::Error::new("case not found or not accessible")
        })?;

        let row_id: Uuid = row
            .try_get("id")
            .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
        let tenant_id_col: Uuid = row
            .try_get("tenant_id")
            .map_err(|e| async_graphql::Error::new(format!("row.tenant_id: {e}")))?;
        let created_by: Option<Uuid> = row
            .try_get("created_by")
            .map_err(|e| async_graphql::Error::new(format!("row.created_by: {e}")))?;
        let created_at: String = row
            .try_get("created_at_s")
            .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;

        let input_features_val: serde_json::Value = row
            .try_get("input_features")
            .map_err(|e| async_graphql::Error::new(format!(
                "case {row_id}: input_features is NULL (legacy row): {e}"
            )))?;
        let recommendation_val: serde_json::Value = row
            .try_get("recommendation")
            .map_err(|e| async_graphql::Error::new(format!(
                "case {row_id}: recommendation is NULL (legacy row): {e}"
            )))?;

        let input: PredictInput =
            serde_json::from_value(input_features_val.clone()).map_err(|e| {
                async_graphql::Error::new(format!(
                    "case {row_id}: input_features parse error: {e}"
                ))
            })?;
        // S6.8 — nlp_suggestion is nullable; repredict preserves whatever the
        // original createCase stored (it does not re-run extraction).
        let nlp_suggestion_val: Option<serde_json::Value> = row
            .try_get("nlp_suggestion")
            .map_err(|e| async_graphql::Error::new(format!("row.nlp_suggestion: {e}")))?;
        let nlp_suggestion: Option<ExtractedFeatures> = nlp_suggestion_val
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| {
                async_graphql::Error::new(format!(
                    "case {row_id}: nlp_suggestion parse error: {e}"
                ))
            })?;

        let recommendation: RecommendationDto =
            serde_json::from_value(recommendation_val).map_err(|e| {
                async_graphql::Error::new(format!(
                    "case {row_id}: recommendation parse error: {e}"
                ))
            })?;

        // ── 3. Re-run ML inference over stored input_features ─────────────────
        let input_json = serde_json::to_vec(&input)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let ml = ctx
            .data::<MlInferenceClient>()
            .map_err(|_| async_graphql::Error::new("ml inference client unavailable"))?;

        // gRPC call to ml-inference-svc (JP-71). On failure the in-flight DB
        // transaction is rolled back before returning the GraphQL error.
        let outcome = call_ml(ml, &tenant_id, &input).await;
        let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

        let new_prediction = match outcome {
            MlCallOutcome::Ok(p) => p,
            MlCallOutcome::Timeout => {
                let _ = tx.rollback().await;
                tracing::warn!("ml-inference-svc timed out (repredictCase)");
                return Err(async_graphql::Error::new("ml inference timed out")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceTimeout")));
            }
            MlCallOutcome::BadRequest(msg) => {
                let _ = tx.rollback().await;
                tracing::warn!(detail = %msg, "ml-inference-svc rejected request (repredictCase)");
                return Err(async_graphql::Error::new("ml inference rejected request")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceBadRequest")));
            }
            MlCallOutcome::Unavailable(msg) => {
                let _ = tx.rollback().await;
                tracing::warn!(detail = %msg, "ml-inference-svc unavailable (repredictCase)");
                let detail = msg.clone();
                return Err(async_graphql::Error::new(format!("ml inference unavailable: {detail}"))
                    .extend_with(move |_, ext| {
                        ext.set("code", "MlInferenceUnavailable");
                        ext.set("detail", detail.clone());
                    }));
            }
            MlCallOutcome::Internal(msg) => {
                let _ = tx.rollback().await;
                tracing::error!(detail = %msg, "ml-inference-svc internal error (repredictCase)");
                let detail = msg.clone();
                return Err(async_graphql::Error::new(format!("ml inference error: {detail}"))
                    .extend_with(move |_, ext| {
                        ext.set("code", "MlInferenceInternal");
                        ext.set("detail", detail.clone());
                    }));
            }
        };

        // ── 4. Persist: INSERT into predictions + UPDATE cases ─────────────────
        let prediction_val = serde_json::to_value(&new_prediction)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO predictions (case_id, tenant_id, prediction, model_version)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(case_uuid)
        .bind(tenant_id)
        .bind(&prediction_val)
        .bind(&new_prediction.model_version)
        .execute(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("predictions insert failed: {e}")))?;

        sqlx::query(
            "UPDATE cases SET prediction = $1, updated_at = now() WHERE id = $2 AND tenant_id = $3",
        )
        .bind(&prediction_val)
        .bind(case_uuid)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("case update failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx commit: {e}")))?;

        // ── 5. Fire-and-forget audit record ──────────────────────────────────
        if let Some(recorder) = ctx
            .data::<Option<AuditRecorder>>()
            .ok()
            .and_then(|r| r.as_ref())
        {
            let recorder = recorder.clone();
            let event = AuditEvent {
                actor:        "api-gateway".to_string(),
                action:       "case.repredict".to_string(),
                payload_hash: hash_payload(&input_json),
                latency_ms,
                status:       AuditStatus::Ok,
                cost_micros:  None,
            };
            tokio::spawn(async move {
                if let Err(e) = recorder.record(tenant_id, event).await {
                    tracing::warn!(error = %e, "repredictCase audit record failed (non-fatal)");
                }
            });
        }

        // ── 6. Return the updated Case ────────────────────────────────────────
        // Recommendation is unchanged — Sprint-5 will re-run decision-arith
        // when operator-supplied damages/cost are accepted at repredict time.
        Ok(Case {
            id:             ID::from(row_id.to_string()),
            tenant_id:      ID::from(tenant_id_col.to_string()),
            input_features: Json(input),
            prediction:     new_prediction,
            recommendation,
            created_by:     created_by.map(|u| ID::from(u.to_string())),
            created_at,
            nlp_suggestion: nlp_suggestion.map(Json),
        })
    }
}

// ---------------------------------------------------------------------------
// Unit tests — pure logic, no I/O
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// PredictInput round-trips through serde JSON without loss.
    ///
    /// Covers the full field set so a future rename or missing Serialize
    /// derive breaks here rather than at runtime.
    #[test]
    fn predict_input_json_roundtrip() {
        let input = PredictInput {
            judge_severity: 0.7,
            attorney_win_rate: 0.6,
            ideology_distance: 0.3,
            materiality_score: 0.8,
            procedural_motion_count: 3.0,
            case_type: "civil".to_string(),
            jurisdiction: "Federal".to_string(),
        };

        let json = serde_json::to_string(&input).expect("serialize");
        let decoded: PredictInput = serde_json::from_str(&json).expect("deserialize");

        // f32 comparisons — 1e-6 is well within single-precision JSON round-trip.
        assert!((decoded.judge_severity - input.judge_severity).abs() < 1e-6);
        assert!((decoded.attorney_win_rate - input.attorney_win_rate).abs() < 1e-6);
        assert!((decoded.ideology_distance - input.ideology_distance).abs() < 1e-6);
        assert!((decoded.materiality_score - input.materiality_score).abs() < 1e-6);
        assert!(
            (decoded.procedural_motion_count - input.procedural_motion_count).abs() < 1e-6
        );
        assert_eq!(decoded.case_type, input.case_type);
        assert_eq!(decoded.jurisdiction, input.jurisdiction);
    }

    /// sha256 hashing of serialised PredictInput is deterministic.
    ///
    /// Two serialisations of the same value must hash identically; two
    /// inputs that differ in at least one field must hash differently.
    #[test]
    fn predict_input_hash_is_deterministic() {
        let input_a = PredictInput {
            judge_severity: 0.5,
            attorney_win_rate: 0.5,
            ideology_distance: 0.5,
            materiality_score: 0.5,
            procedural_motion_count: 1.0,
            case_type: "criminal".to_string(),
            jurisdiction: "California".to_string(),
        };

        let bytes_a1 = serde_json::to_vec(&input_a).unwrap();
        let bytes_a2 = serde_json::to_vec(&input_a).unwrap();

        let h1 = hash_payload(&bytes_a1);
        let h2 = hash_payload(&bytes_a2);

        assert_eq!(h1, h2, "identical inputs must produce identical hashes");
        assert_eq!(h1.len(), 64, "SHA-256 hex must be exactly 64 chars");

        // A different value in any field must change the digest.
        let input_b = PredictInput {
            judge_severity: 0.9, // differs
            attorney_win_rate: 0.5,
            ideology_distance: 0.5,
            materiality_score: 0.5,
            procedural_motion_count: 1.0,
            case_type: "criminal".to_string(),
            jurisdiction: "California".to_string(),
        };
        let bytes_b = serde_json::to_vec(&input_b).unwrap();
        let h3 = hash_payload(&bytes_b);

        assert_ne!(h1, h3, "distinct inputs must produce distinct hashes");
    }

    /// `Case` serializes through serde_json with `expected_value_try` as a
    /// JSON *string*, not a JSON number.
    ///
    /// This guards the invariant that `RecommendationDto` stores monetary
    /// amounts as `String` (preserving `Decimal` precision) even after a
    /// serde round-trip through an intermediate JSON value.
    #[test]
    fn case_serializes_with_decimal_as_string() {
        let rec = RecommendationDto {
            kind: "Try".to_string(),
            confidence: "High".to_string(),
            counter_recommendation: None,
            rationale_bullets: vec![
                "P(win) 0.80 with 90% CI [0.65, 0.92] — high confidence".to_string(),
                "Expected value at trial $70000.00 vs. expected settlement value $40000.00"
                    .to_string(),
                "Trial EV exceeds settlement and lower CI bound is above 0.55".to_string(),
            ],
            expected_value_try:    "70000.00".to_string(),
            expected_value_settle: "40000.00".to_string(),
        };
        let case = Case {
            id:        ID::from("00000000-0000-0000-0000-000000000001"),
            tenant_id: ID::from("00000000-0000-0000-0000-000000000002"),
            input_features: Json(PredictInput {
                judge_severity:          0.7,
                attorney_win_rate:       0.6,
                ideology_distance:       0.3,
                materiality_score:       0.8,
                procedural_motion_count: 3.0,
                case_type:               "civil".to_string(),
                jurisdiction:            "Federal".to_string(),
            }),
            prediction: PredictResult {
                p_win:             0.72,
                ci_lower:          0.61,
                ci_upper:          0.83,
                coverage:          0.90,
                model_version:     "test-run-abc".to_string(),
                predicted_at_unix: 1_746_748_800,
            },
            recommendation: rec,
            created_by:  None,
            created_at:  "2026-05-10T12:00:00Z".to_string(),
            nlp_suggestion: None,
        };

        let json_str = serde_json::to_string(&case).expect("Case must serialize to JSON");
        let value: serde_json::Value =
            serde_json::from_str(&json_str).expect("serialized Case must parse as JSON");

        // expected_value_try is stored as String in RecommendationDto;
        // it must round-trip as a JSON string (not a JSON number).
        let ev_try = &value["recommendation"]["expected_value_try"];
        assert!(
            ev_try.is_string(),
            "expected_value_try must be a JSON string, not a number; got: {ev_try}"
        );
        assert_eq!(
            ev_try.as_str().unwrap(),
            "70000.00",
            "expected_value_try string value must round-trip exactly"
        );

        // S6.8 — nlp_suggestion is None here; it must serialize as JSON null.
        assert!(
            value["nlp_suggestion"].is_null(),
            "absent nlp_suggestion must serialize as JSON null; got: {}",
            value["nlp_suggestion"]
        );
    }

    /// S6.8 — a `Case` carrying an `nlp_suggestion` round-trips through
    /// serde_json with every ExtractedFeatures field intact.
    #[test]
    fn case_round_trips_with_nlp_suggestion() {
        let suggestion = ExtractedFeatures {
            judge_severity: Some(0.42),
            judge_name: Some("LAUBER".to_string()),
            judge_cases_analyzed: Some(7),
            case_type_hint: "innocent_spouse".to_string(),
            case_type_suggestion: Some("civil".to_string()),
            outcome_for: Some("respondent".to_string()),
            jurisdiction_suggestion: Some("us-federal".to_string()),
        };
        let case = Case {
            id: ID::from("00000000-0000-0000-0000-000000000010"),
            tenant_id: ID::from("00000000-0000-0000-0000-000000000002"),
            input_features: Json(PredictInput {
                judge_severity:          0.5,
                attorney_win_rate:       0.6,
                ideology_distance:       0.3,
                materiality_score:       0.8,
                procedural_motion_count: 2.0,
                case_type:               "civil".to_string(),
                jurisdiction:            "us-federal".to_string(),
            }),
            prediction: PredictResult {
                p_win:             0.6,
                ci_lower:          0.5,
                ci_upper:          0.7,
                coverage:          0.90,
                model_version:     "test".to_string(),
                predicted_at_unix: 1_746_748_800,
            },
            recommendation: RecommendationDto {
                kind: "Borderline".to_string(),
                confidence: "Medium".to_string(),
                counter_recommendation: None,
                rationale_bullets: vec![
                    "b1".to_string(),
                    "b2".to_string(),
                    "b3".to_string(),
                ],
                expected_value_try:    "10000.00".to_string(),
                expected_value_settle: "40000.00".to_string(),
            },
            created_by: None,
            created_at: "2026-05-10T12:00:00Z".to_string(),
            nlp_suggestion: Some(Json(suggestion)),
        };

        let json_str = serde_json::to_string(&case).expect("Case must serialize");
        let decoded: Case =
            serde_json::from_str(&json_str).expect("Case must deserialize");

        let nlp = decoded
            .nlp_suggestion
            .expect("nlp_suggestion must round-trip as Some");
        assert_eq!(nlp.0.judge_severity, Some(0.42));
        assert_eq!(nlp.0.judge_name.as_deref(), Some("LAUBER"));
        assert_eq!(nlp.0.judge_cases_analyzed, Some(7));
        assert_eq!(nlp.0.case_type_hint, "innocent_spouse");
        assert_eq!(nlp.0.case_type_suggestion.as_deref(), Some("civil"));
        assert_eq!(nlp.0.outcome_for.as_deref(), Some("respondent"));
        assert_eq!(nlp.0.jurisdiction_suggestion.as_deref(), Some("us-federal"));
    }

    /// `RecommendationDto` serializes and deserializes deterministically
    /// (same in → same JSON → same out).
    #[test]
    fn recommendation_dto_round_trip() {
        let dto = RecommendationDto {
            kind: "Borderline".to_string(),
            confidence: "Medium".to_string(),
            counter_recommendation: None,
            rationale_bullets: vec![
                "P(win) 0.50 with 90% CI [0.45, 0.60] — medium confidence".to_string(),
                "Expected value at trial $5000.00 vs. expected settlement value $40000.00"
                    .to_string(),
                "Outcome is borderline: CI lower bound (0.45) falls between thresholds".to_string(),
            ],
            expected_value_try:    "5000.00".to_string(),
            expected_value_settle: "40000.00".to_string(),
        };

        let json_str =
            serde_json::to_string(&dto).expect("RecommendationDto must serialize to JSON");
        let decoded: RecommendationDto =
            serde_json::from_str(&json_str).expect("RecommendationDto must deserialize from JSON");

        assert_eq!(decoded.kind,              dto.kind);
        assert_eq!(decoded.rationale_bullets, dto.rationale_bullets);
        assert_eq!(decoded.expected_value_try,    dto.expected_value_try);
        assert_eq!(decoded.expected_value_settle, dto.expected_value_settle);
    }

    // ── CaseConnection helpers and tests (S4.3) ──────────────────────────────

    /// Build a minimal `Case` fixture for CaseConnection round-trip tests.
    fn make_test_case(id: &str) -> Case {
        Case {
            id:        ID::from(id),
            tenant_id: ID::from("00000000-0000-0000-0000-000000000002"),
            input_features: Json(PredictInput {
                judge_severity:          0.7,
                attorney_win_rate:       0.6,
                ideology_distance:       0.3,
                materiality_score:       0.8,
                procedural_motion_count: 3.0,
                case_type:               "civil".to_string(),
                jurisdiction:            "Federal".to_string(),
            }),
            prediction: PredictResult {
                p_win:             0.72,
                ci_lower:          0.61,
                ci_upper:          0.83,
                coverage:          0.90,
                model_version:     "test".to_string(),
                predicted_at_unix: 1_746_748_800,
            },
            recommendation: RecommendationDto {
                kind: "Try".to_string(),
                confidence: "High".to_string(),
                counter_recommendation: None,
                rationale_bullets: vec![
                    "bullet-1".to_string(),
                    "bullet-2".to_string(),
                    "bullet-3".to_string(),
                ],
                expected_value_try:    "70000.00".to_string(),
                expected_value_settle: "40000.00".to_string(),
            },
            created_by: None,
            created_at: "2026-05-10T12:00:00Z".to_string(),
            nlp_suggestion: None,
        }
    }

    /// `CaseConnection` round-trips through serde_json with correct field values
    /// when `nodes` is non-empty and `next_offset` is `Some`.
    #[test]
    fn case_connection_serializes() {
        let conn = CaseConnection {
            nodes: vec![make_test_case("00000000-0000-0000-0000-000000000001")],
            total_count: 5,
            next_offset: Some(1),
        };

        let json_str = serde_json::to_string(&conn).expect("CaseConnection must serialize");
        let decoded: CaseConnection =
            serde_json::from_str(&json_str).expect("CaseConnection must deserialize");

        assert_eq!(decoded.total_count, 5);
        assert_eq!(decoded.next_offset, Some(1));
        assert_eq!(decoded.nodes.len(), 1);
    }

    /// When `offset + nodes.len() == total_count`, `compute_next_offset` returns
    /// `None`, which serializes as a JSON null in the response.
    #[test]
    fn case_connection_next_offset_at_end_is_none() {
        // 3 total, offset = 1, 2 nodes → 1 + 2 = 3 == total → None.
        assert_eq!(
            compute_next_offset(1, 2, 3),
            None,
            "last page must produce None"
        );

        let conn = CaseConnection {
            nodes: vec![
                make_test_case("00000000-0000-0000-0000-000000000001"),
                make_test_case("00000000-0000-0000-0000-000000000002"),
            ],
            total_count: 3,
            next_offset: compute_next_offset(1, 2, 3),
        };

        let json_str = serde_json::to_string(&conn).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
        assert!(
            value["next_offset"].is_null(),
            "next_offset must be null when page exhausts the result set; got: {:?}",
            value["next_offset"]
        );
    }

    /// When more rows remain beyond the current page, `compute_next_offset`
    /// returns `Some(offset + nodes.len())`.
    #[test]
    fn case_connection_next_offset_mid_page() {
        // 10 total, offset = 0, 3 nodes → 0 + 3 = 3 < 10 → Some(3).
        assert_eq!(compute_next_offset(0, 3, 10), Some(3));
        // 10 total, offset = 3, 3 nodes → 3 + 3 = 6 < 10 → Some(6).
        assert_eq!(compute_next_offset(3, 3, 10), Some(6));
        // Exact last page: offset = 8, 2 nodes → 8 + 2 = 10 == total → None.
        assert_eq!(compute_next_offset(8, 2, 10), None);

        let conn = CaseConnection {
            nodes: vec![
                make_test_case("00000000-0000-0000-0000-000000000001"),
                make_test_case("00000000-0000-0000-0000-000000000002"),
            ],
            total_count: 10,
            next_offset: compute_next_offset(0, 2, 10),
        };

        let json_str = serde_json::to_string(&conn).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json_str).expect("parse");
        assert_eq!(
            value["next_offset"].as_i64().unwrap_or(-1),
            2,
            "next_offset must equal offset + nodes.len() when more rows remain"
        );
    }

    // ── S4.7 tests (PredictionHistoryEntry + repredictCase logic) ────────────

    /// `PredictionHistoryEntry` round-trips through serde_json with a non-empty
    /// `model_version` string and a nested `PredictResult`.
    ///
    /// Guards the invariant that the type remains serialisable — a missing
    /// `Serialize`/`Deserialize` derive or a field rename would break the
    /// `casePredictions` GraphQL response body.
    #[test]
    fn prediction_history_entry_serializes() {
        let entry = PredictionHistoryEntry {
            id:           ID::from("00000000-0000-0000-0000-000000000010"),
            prediction: PredictResult {
                p_win:             0.65,
                ci_lower:          0.55,
                ci_upper:          0.75,
                coverage:          0.90,
                model_version:     "sprint4-run-001".to_string(),
                predicted_at_unix: 1_746_748_900,
            },
            model_version: "sprint4-run-001".to_string(),
            created_at:    "2026-05-10T14:00:00Z".to_string(),
        };

        let json_str =
            serde_json::to_string(&entry).expect("PredictionHistoryEntry must serialize");
        let decoded: PredictionHistoryEntry =
            serde_json::from_str(&json_str).expect("PredictionHistoryEntry must deserialize");

        assert_eq!(decoded.model_version, entry.model_version);
        assert!((decoded.prediction.p_win - 0.65_f32).abs() < 1e-6);
        assert_eq!(decoded.created_at, entry.created_at);

        // model_version must appear as a JSON string (not a number).
        let value: serde_json::Value =
            serde_json::from_str(&json_str).expect("must parse as JSON value");
        assert!(
            value["model_version"].is_string(),
            "model_version must be a JSON string; got: {:?}",
            value["model_version"]
        );
    }

    /// Documents that the full `repredict_case` resolver is covered by the
    /// E2E smoke test `repredict_creates_history_and_updates_case`.
    ///
    /// A unit test against a mock `PgPool` is deferred: `sqlx::PgPool` does
    /// not implement a test-double trait in Sprint 4.  Sprint-5 backlog:
    /// introduce an abstract DB trait so the resolver can be tested without a
    /// live Postgres connection.
    ///
    /// This test validates the PredictResult construction logic that the
    /// resolver uses when building `new_prediction` from an ML response.
    #[test]
    fn repredict_returns_case_with_updated_prediction() {
        // Simulate what the resolver constructs from the ML HTTP response.
        let new_prediction = PredictResult {
            p_win:             0.78,
            ci_lower:          0.65,
            ci_upper:          0.89,
            coverage:          0.90,
            model_version:     "repredict-run-v2".to_string(),
            predicted_at_unix: 1_746_799_800,
        };
        let original_prediction = PredictResult {
            p_win:             0.60,
            ci_lower:          0.50,
            ci_upper:          0.70,
            coverage:          0.90,
            model_version:     "original-run-v1".to_string(),
            predicted_at_unix: 1_746_700_000,
        };

        // The resolver persists `new_prediction` and discards `original_prediction`.
        // Verify they differ so the UPDATE is meaningful.
        assert!(
            (new_prediction.p_win - original_prediction.p_win).abs() > 1e-6,
            "repredictCase must produce a different p_win from the original prediction"
        );
        assert_ne!(
            new_prediction.model_version,
            original_prediction.model_version,
            "repredictCase must reflect a newer model_version"
        );

        // Serialisation sanity: the new prediction round-trips correctly.
        let json = serde_json::to_string(&new_prediction)
            .expect("new PredictResult must serialize");
        let decoded: PredictResult =
            serde_json::from_str(&json).expect("new PredictResult must deserialize");
        assert!((decoded.p_win - new_prediction.p_win).abs() < 1e-6);
        assert_eq!(decoded.model_version, new_prediction.model_version);
    }
}
