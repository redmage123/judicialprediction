//! S6.15 — Public REST API.
//!
//! Single endpoint in v1: `POST /v1/cases`, mirroring the GraphQL
//! `createCase` mutation.  Same RLS, same ML / decision-arith / INSERT /
//! audit pipeline — both routes call `case_import::do_create_case`.
//!
//! Auth: any `Authorization: Bearer <token>` the gateway accepts.  JWTs
//! work for browser-driven sessions, `pat_*` tokens for server-to-server
//! integrations.  The `jwt_middleware` layer has already injected
//! `Claims` + `TenantId` into request extensions by the time the handler
//! is reached.
//!
//! Out of scope for v1 (Sprint-7 candidates):
//! - OpenAPI / Swagger UI at `/api/docs` (S6.18).
//! - Per-PAT rate-limit override above the shared per-tenant bucket (S6.19).
//! - REST endpoints beyond `POST /v1/cases` (S6.20+).

use std::sync::Arc;

use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Claims;
use crate::case_import::do_create_case;
use crate::graphql_predict::{Case, PredictInput};

/// Request body for `POST /v1/cases`.  snake_case fields to follow REST
/// convention (the GraphQL InputObject uses camelCase per async-graphql's
/// default naming, but `PredictInput`'s serde uses field names verbatim,
/// which are already snake_case).
#[derive(Deserialize)]
pub struct CreateCaseV1Request {
    #[serde(flatten)]
    pub input: PredictInput,
    /// Optional raw opinion text — same semantics as `createCase`'s
    /// `opinionText` GraphQL arg.
    #[serde(default)]
    pub opinion_text: Option<String>,
}

/// Public REST response — flattens GraphQL `Case` to plain JSON.  We
/// deliberately do NOT re-export the GraphQL ID type; everything is
/// `String` (UUIDs) or the underlying primitives.
#[derive(Debug, Serialize)]
pub struct CaseResponseV1 {
    pub id: String,
    pub tenant_id: String,
    pub input_features: serde_json::Value,
    pub prediction: serde_json::Value,
    pub recommendation: serde_json::Value,
    pub created_by: Option<String>,
    pub created_at: String,
    pub nlp_suggestion: Option<serde_json::Value>,
}

impl From<Case> for CaseResponseV1 {
    fn from(c: Case) -> Self {
        Self {
            id:              c.id.to_string(),
            tenant_id:       c.tenant_id.to_string(),
            input_features:  serde_json::to_value(&c.input_features.0).unwrap_or(serde_json::Value::Null),
            prediction:      serde_json::to_value(&c.prediction).unwrap_or(serde_json::Value::Null),
            recommendation:  serde_json::to_value(&c.recommendation).unwrap_or(serde_json::Value::Null),
            created_by:      c.created_by.map(|id| id.to_string()),
            created_at:      c.created_at,
            nlp_suggestion:  c.nlp_suggestion.map(|j| serde_json::to_value(&j.0).unwrap_or(serde_json::Value::Null)),
        }
    }
}

/// Closed error-code union the REST surface returns on failure.  Each
/// variant maps 1:1 to an HTTP status; the body is `{"code": "...",
/// "message": "..."}` so SDK code can branch on `code` without parsing
/// human text.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: &'static str,
    pub message: String,
}

impl ErrorResponse {
    fn into_response(self, status: StatusCode) -> axum::response::Response {
        (status, Json(self)).into_response()
    }
}

/// `POST /v1/cases` — create + predict + persist one case.  Claims are
/// guaranteed to be in extensions because the auth middleware short-
/// circuits with 401 otherwise.
pub async fn create_case_v1(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<CreateCaseV1Request>,
) -> Result<axum::response::Response, axum::response::Response> {
    let Some(pool) = state.cases_pool.as_ref() else {
        return Err(ErrorResponse {
            code:    "store_not_configured",
            message: "cases store is not configured on this gateway".to_string(),
        }
        .into_response(StatusCode::SERVICE_UNAVAILABLE));
    };

    let tenant_id = Uuid::parse_str(&claims.tenant_id).map_err(|_| {
        ErrorResponse {
            code:    "invalid_tenant_id",
            message: "tenant_id in token is not a valid UUID".to_string(),
        }
        .into_response(StatusCode::UNAUTHORIZED)
    })?;
    let operator_id: Option<Uuid> = Uuid::parse_str(&claims.sub).ok();

    let case = do_create_case(
        pool,
        &state.ml_client,
        state.audit_recorder.clone(),
        tenant_id,
        operator_id,
        body.input,
        body.opinion_text,
    )
    .await
    .map_err(|e| {
        // Re-use the closed code-set from do_create_case's GraphQL error
        // extensions where present; default to "internal" otherwise.
        let code = e
            .extensions
            .as_ref()
            .and_then(|ext| ext.get("code"))
            .and_then(|v| match v {
                async_graphql::Value::String(s) => Some(s.as_str()),
                _ => None,
            })
            .map(map_internal_code)
            .unwrap_or("internal");
        let status = match code {
            "MlInferenceBadRequest" => StatusCode::BAD_REQUEST,
            "MlInferenceTimeout" => StatusCode::GATEWAY_TIMEOUT,
            "MlInferenceUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
            "MlInferenceInternal" => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ErrorResponse {
            code: Box::leak(code.to_string().into_boxed_str()),
            message: e.message,
        }
        .into_response(status)
    })?;

    Ok((StatusCode::OK, Json(CaseResponseV1::from(case))).into_response())
}

fn map_internal_code(code: &str) -> &str {
    // Pass-through today; kept as a translation seam so we can rename the
    // GraphQL extension codes without leaking them into the REST contract.
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_response_serializes_with_snake_case_id() {
        let c = CaseResponseV1 {
            id:              "550e8400-e29b-41d4-a716-446655440000".to_string(),
            tenant_id:       "00000000-0000-0000-0000-000000000001".to_string(),
            input_features:  serde_json::json!({"case_type": "civil"}),
            prediction:      serde_json::json!({"p_win": 0.7}),
            recommendation:  serde_json::json!({"kind": "Settle"}),
            created_by:      Some("op-1".to_string()),
            created_at:      "2026-05-16T00:00:00Z".to_string(),
            nlp_suggestion:  None,
        };
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        assert!(v.get("id").is_some());
        assert!(v.get("tenant_id").is_some());
        assert!(v.get("input_features").is_some());
        assert!(v.get("nlp_suggestion").unwrap().is_null());
    }

    #[test]
    fn error_response_has_code_and_message_fields() {
        let e = ErrorResponse { code: "bad_input", message: "judge_severity out of range".to_string() };
        let v: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "bad_input");
        assert_eq!(v["message"], "judge_severity out of range");
    }
}
