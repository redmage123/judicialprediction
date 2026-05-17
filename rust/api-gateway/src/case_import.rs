//! Bulk case import — backs the S6.14 `importCases` GraphQL mutation.
//!
//! Sync per-row pipeline: for each `ImportCaseRow` the gateway runs the
//! same ML inference + decision-arith + INSERT + audit flow the
//! `createCase` mutation runs.  The per-row contract is intentionally
//! identical so an imported case is indistinguishable from one created
//! through the intake form — same row shape, same RLS, same audit trail.
//!
//! The current request cap is 50 rows.  Larger imports will get an
//! asynchronous `case_imports` queue in a Sprint-7 follow-up; the cap
//! protects the gateway from a long-running synchronous mutation in v1.
//!
//! NOTE — code duplication with `graphql_predict::create_case`: the
//! per-row helper [`do_create_case`] re-implements `create_case`'s body
//! line-for-line.  De-duplication is deliberately deferred (S6.17) so the
//! battle-tested `create_case` path is not refactored under the same
//! sprint that ships bulk import.  Touch both when you change one.

use std::sync::Arc;
use std::time::Instant;

use async_graphql::{ErrorExtensions, ID, InputObject, Json, SimpleObject};
use audit_recorder::{AuditEvent, AuditRecorder, AuditStatus, hash_payload};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row as _};
use uuid::Uuid;

use crate::graphql_predict::{
    Case, ExtractedFeatures, MlCallOutcome, MlInferenceClient, PredictInput,
    build_recommendation_dto, call_ml, extract_features_from_text,
};

/// Hard cap on rows per `importCases` request.  See the module docstring.
pub const MAX_IMPORT_ROWS: usize = 50;

/// GraphQL InputObject for one CSV row.  Flat shape — matches what the web
/// frontend produces from papaparse without an intermediate `PredictInput`
/// nesting layer (simpler to validate row-by-row in the CSV preview).
#[derive(Debug, Clone, InputObject, Serialize, Deserialize)]
pub struct ImportCaseRow {
    pub judge_severity: f32,
    pub attorney_win_rate: f32,
    pub ideology_distance: f32,
    pub materiality_score: f32,
    pub procedural_motion_count: f32,
    pub case_type: String,
    pub jurisdiction: String,
    /// Optional raw opinion text; treated identically to `createCase`'s
    /// `opinionText` arg — when present, the NLP suggestion is persisted
    /// in `cases.nlp_suggestion`.
    pub opinion_text: Option<String>,
}

impl ImportCaseRow {
    fn into_predict_input(self) -> (PredictInput, Option<String>) {
        (
            PredictInput {
                judge_severity:           self.judge_severity,
                attorney_win_rate:        self.attorney_win_rate,
                ideology_distance:        self.ideology_distance,
                materiality_score:        self.materiality_score,
                procedural_motion_count:  self.procedural_motion_count,
                case_type:                self.case_type,
                jurisdiction:             self.jurisdiction,
            },
            self.opinion_text,
        )
    }
}

/// Per-row outcome inside [`ImportCasesResult::results`].
#[derive(Debug, Clone, SimpleObject, Serialize, Deserialize)]
pub struct ImportRowResult {
    pub row_index: u32,
    pub ok: bool,
    pub case_id: Option<ID>,
    /// Operator-facing error message when `ok = false`.  No internal
    /// stack traces — the same closed error-code-style messages the
    /// `createCase` mutation produces (`ml inference timed out`, etc.).
    pub error: Option<String>,
}

/// Aggregate result for one `importCases` mutation call.
#[derive(Debug, Clone, SimpleObject, Serialize, Deserialize)]
pub struct ImportCasesResult {
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub results: Vec<ImportRowResult>,
}

/// Sync per-row pipeline: ML inference → decision-arith → INSERT → audit.
///
/// Mirrors [`graphql_predict::Mutation::create_case`] exactly — same
/// errors, same RLS pattern, same audit semantics.  Identity (tenant +
/// operator) is supplied by the caller; both `createCase` and
/// `importCases` extract it from Claims once.  Audit recorder is taken
/// by value because each row spawns its own `tokio::spawn` task.
pub async fn do_create_case(
    pool: &PgPool,
    ml: &MlInferenceClient,
    audit_recorder: Option<AuditRecorder>,
    tenant_id: Uuid,
    operator_id: Option<Uuid>,
    input: PredictInput,
    opinion_text: Option<String>,
) -> async_graphql::Result<Case> {
    let start = Instant::now();

    let input_json = serde_json::to_vec(&input)
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

    let outcome = call_ml(ml, &tenant_id, &input).await;
    let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

    let prediction = match outcome {
        MlCallOutcome::Ok(p) => p,
        MlCallOutcome::Timeout => {
            return Err(async_graphql::Error::new("ml inference timed out")
                .extend_with(|_, ext| ext.set("code", "MlInferenceTimeout")));
        }
        MlCallOutcome::BadRequest(_msg) => {
            return Err(async_graphql::Error::new("ml inference rejected request")
                .extend_with(|_, ext| ext.set("code", "MlInferenceBadRequest")));
        }
        MlCallOutcome::Unavailable(msg) => {
            let detail = msg.clone();
            return Err(async_graphql::Error::new(format!("ml inference unavailable: {detail}"))
                .extend_with(move |_, ext| {
                    ext.set("code", "MlInferenceUnavailable");
                    ext.set("detail", detail.clone());
                }));
        }
        MlCallOutcome::Internal(msg) => {
            let detail = msg.clone();
            return Err(async_graphql::Error::new(format!("ml inference error: {detail}"))
                .extend_with(move |_, ext| {
                    ext.set("code", "MlInferenceInternal");
                    ext.set("detail", detail.clone());
                }));
        }
    };

    let decision_input = decision_arith::PredictionInput {
        p_win:            f64::from(prediction.p_win),
        ci_lower:         f64::from(prediction.ci_lower),
        ci_upper:         f64::from(prediction.ci_upper),
        expected_damages: Decimal::from(100_000u32),
    };
    let motion_count = input.procedural_motion_count.max(0.0).round() as u32;
    let cost = cost_engine::estimate_cost_v2(&cost_engine::CostInputs {
        jurisdiction:             &input.jurisdiction,
        motion_count,
        expected_duration_months: cost_engine::derive_duration_months(motion_count),
        party_count:              cost_engine::BASELINE_PARTY_COUNT,
    });
    let rec = decision_arith::recommend(&decision_input, cost, &input.jurisdiction);
    let recommendation = build_recommendation_dto(rec);

    let input_features_val = serde_json::to_value(&input)
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
    let prediction_val = serde_json::to_value(&prediction)
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;
    let recommendation_val = serde_json::to_value(&recommendation)
        .map_err(|e| async_graphql::Error::new(e.to_string()))?;

    let nlp_suggestion: Option<ExtractedFeatures> = match opinion_text.as_deref() {
        Some(text) if !text.trim().is_empty() => {
            // Sprint-10: bulk import has no per-row as-of-date; resolver
            // uses the MQ latest snapshot.
            Some(extract_features_from_text(pool, tenant_id, text, None).await?)
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

    sqlx::query(&format!(
        "SET LOCAL app.current_tenant_id = '{tenant_id}'"
    ))
    .execute(&mut *tx)
    .await
    .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

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

    if let Some(recorder) = audit_recorder {
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
                tracing::warn!(error = %e, "importCases audit record failed (non-fatal)");
            }
        });
    }

    Ok(Case {
        id:             ID::from(case_id.to_string()),
        tenant_id:      ID::from(tenant_id.to_string()),
        input_features: Json(input),
        prediction,
        recommendation,
        created_by:     operator_id.map(|id| ID::from(id.to_string())),
        created_at:     created_at_s,
        nlp_suggestion: nlp_suggestion.map(Json),
        // S10.4 — importCases doesn't persist provenance yet (bulk path
        // shares the gateway resolver but the per-row insert in
        // case_import.rs doesn't pipe it through). Sprint 11 candidate.
        ideology_provenance: None,
    })
}

/// Orchestrator for the bulk import — bounded loop calling [`do_create_case`]
/// per row, aggregating per-row outcomes.  Caller is responsible for
/// resolving the tenant/operator identity before invoking this.
pub async fn do_import_cases(
    pool: &PgPool,
    ml: &MlInferenceClient,
    audit_recorder: Option<AuditRecorder>,
    tenant_id: Uuid,
    operator_id: Option<Uuid>,
    rows: Vec<ImportCaseRow>,
) -> async_graphql::Result<ImportCasesResult> {
    if rows.is_empty() {
        return Err(async_graphql::Error::new("rows must not be empty")
            .extend_with(|_, ext| ext.set("code", "EmptyImport")));
    }
    if rows.len() > MAX_IMPORT_ROWS {
        return Err(async_graphql::Error::new(format!(
            "too many rows ({} > {}); bulk import is limited per request",
            rows.len(),
            MAX_IMPORT_ROWS
        ))
        .extend_with(|_, ext| ext.set("code", "TooManyRows")));
    }

    let mut results = Vec::with_capacity(rows.len());
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    for (idx, row) in rows.into_iter().enumerate() {
        let (predict_input, opinion_text) = row.into_predict_input();
        match do_create_case(
            pool,
            ml,
            audit_recorder.clone(),
            tenant_id,
            operator_id,
            predict_input,
            opinion_text,
        )
        .await
        {
            Ok(case) => {
                succeeded += 1;
                results.push(ImportRowResult {
                    row_index: idx as u32,
                    ok:        true,
                    case_id:   Some(case.id),
                    error:     None,
                });
            }
            Err(e) => {
                failed += 1;
                results.push(ImportRowResult {
                    row_index: idx as u32,
                    ok:        false,
                    case_id:   None,
                    error:     Some(e.message),
                });
            }
        }
    }

    Ok(ImportCasesResult {
        total: succeeded + failed,
        succeeded,
        failed,
        results,
    })
}

// Wrap Arc<PgPool> for ergonomic Deref where call sites pass it.  Kept
// out of public signatures — the public API takes `&PgPool`.
#[allow(dead_code)]
type _ArcPool = Arc<PgPool>;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(judge_severity: f32, case_type: &str) -> ImportCaseRow {
        ImportCaseRow {
            judge_severity,
            attorney_win_rate:       0.5,
            ideology_distance:       0.3,
            materiality_score:       0.8,
            procedural_motion_count: 3.0,
            case_type:               case_type.to_string(),
            jurisdiction:            "us-federal".to_string(),
            opinion_text:            None,
        }
    }

    #[test]
    fn import_row_converts_to_predict_input_preserving_fields() {
        let r = row(0.42, "civil");
        let (pi, opinion) = r.clone().into_predict_input();
        assert_eq!(pi.judge_severity, 0.42);
        assert_eq!(pi.case_type, "civil");
        assert_eq!(pi.jurisdiction, "us-federal");
        assert!(opinion.is_none());

        let mut with_opinion = r;
        with_opinion.opinion_text = Some("Sample opinion text.".to_string());
        let (_, opinion) = with_opinion.into_predict_input();
        assert_eq!(opinion.as_deref(), Some("Sample opinion text."));
    }

    #[test]
    fn max_import_rows_constant_matches_ticket() {
        // S6.14 spec: sync up to 50 rows per request.
        assert_eq!(MAX_IMPORT_ROWS, 50);
    }

    // Note: end-to-end tests of do_import_cases require a live Postgres
    // pool + ML stub, so they live in the existing api-gateway integration
    // harness rather than unit tests here.  do_create_case's individual
    // branches are covered by the equivalent createCase tests in
    // graphql_predict.rs — the two paths are intentionally identical, and
    // the dedupe ticket (S6.17) will collapse them into one tested code
    // path.
}
