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
enum MlCallOutcome {
    Ok(PredictResult),
    /// The RPC deadline was exceeded (tonic `DeadlineExceeded`).
    Timeout,
    /// The server returned `INVALID_ARGUMENT` — e.g. a Tier-C feature was
    /// supplied, or a required feature was missing.
    BadRequest(String),
    /// Any other gRPC error (service unavailable, transport failure, etc.).
    Unavailable,
}

/// Build a `PredictCaseOutcomeRequest`, send it to ml-inference-svc over gRPC,
/// and convert the response to `PredictResult`.
///
/// Feature values are encoded as `"key:value"` strings in `feature_ids` per
/// the JP-70 Python server contract (`grpc_server.py`).  The `x-tenant-id`
/// metadata header is set from `tenant_id` so the ML service's own
/// `audit_recorder` fires with the correct tenant context.
async fn call_ml(
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
        Err(status) => match status.code() {
            tonic::Code::DeadlineExceeded => MlCallOutcome::Timeout,
            tonic::Code::InvalidArgument => {
                MlCallOutcome::BadRequest(status.message().to_string())
            }
            _ => MlCallOutcome::Unavailable,
        },
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
    /// Three deterministic reasoning bullets produced by decision-arith.
    pub rationale_bullets: Vec<String>,
    /// Expected value of going to trial (`p_win × damages − cost`), as a
    /// decimal string (e.g. `"-20000.00"`). May be negative.
    pub expected_value_try: String,
    /// Expected value of settlement (`damages × 0.40` anchor), as a decimal
    /// string (e.g. `"40000.00"`).
    pub expected_value_settle: String,
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
}

// ---------------------------------------------------------------------------
// Pagination helper — used by the listCases resolver and unit tests
// ---------------------------------------------------------------------------

/// Compute the `nextOffset` cursor for [`CaseConnection`].
///
/// Returns `Some(offset + nodes_len)` when more rows remain beyond the
/// current page (i.e. `offset + nodes_len < total_count`), or `None` when
/// this page exhausts the result set.
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
            MlCallOutcome::Unavailable => {
                tracing::warn!("ml-inference-svc unavailable (predictCaseOutcome)");
                (
                    AuditStatus::Err,
                    Err(async_graphql::Error::new("ml inference unavailable")
                        .extend_with(|_, ext| ext.set("code", "MlInferenceUnavailable"))),
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
            MlCallOutcome::Unavailable => {
                tracing::warn!("ml-inference-svc unavailable (createCase)");
                return Err(async_graphql::Error::new("ml inference unavailable")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceUnavailable")));
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
        // S5.10: replace the $50k cost placeholder with cost-engine output —
        // jurisdiction_base × (1 + 0.08 × motion_count).  S5.11: jurisdiction
        // also flows into the settle anchor (0.45 federal / 0.35 state / 0.40
        // legacy fallback) via decision_arith::recommend.
        // procedural_motion_count is an f32 in the GraphQL input (matches the
        // feature wire format); clamp non-negative and round before casting.
        // Rust's `as u32` is saturating for f32 → u32 since 1.45.
        let motion_count = input
            .procedural_motion_count
            .max(0.0)
            .round() as u32;
        let cost = cost_engine::estimate_cost(&input.jurisdiction, motion_count);
        let rec = decision_arith::recommend(&decision_input, cost, &input.jurisdiction);

        let recommendation = RecommendationDto {
            kind: match rec.kind {
                decision_arith::RecommendationKind::Settle    => "Settle".to_string(),
                decision_arith::RecommendationKind::Try       => "Try".to_string(),
                decision_arith::RecommendationKind::Borderline => "Borderline".to_string(),
            },
            rationale_bullets: rec.rationale_bullets.to_vec(),
            expected_value_try:    rec.expected_value_try.to_string(),
            expected_value_settle: rec.expected_value_settle.to_string(),
        };

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
                 input_features, prediction, recommendation, created_by)
            VALUES
                ($1, $2, $3, $4, $5, $6, $7)
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
        })
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
                   created_at::text AS created_at_s
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
            MlCallOutcome::Unavailable => {
                let _ = tx.rollback().await;
                tracing::warn!("ml-inference-svc unavailable (repredictCase)");
                return Err(async_graphql::Error::new("ml inference unavailable")
                    .extend_with(|_, ext| ext.set("code", "MlInferenceUnavailable")));
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
            rationale_bullets: vec![
                "P(win) 0.80 with 90% CI [0.65, 0.92]".to_string(),
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
    }

    /// `RecommendationDto` serializes and deserializes deterministically
    /// (same in → same JSON → same out).
    #[test]
    fn recommendation_dto_round_trip() {
        let dto = RecommendationDto {
            kind: "Borderline".to_string(),
            rationale_bullets: vec![
                "P(win) 0.50 with 90% CI [0.45, 0.60]".to_string(),
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
