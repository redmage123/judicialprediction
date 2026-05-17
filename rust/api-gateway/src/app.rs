// JudicialPredict API Gateway — application core.
//
// All GraphQL types, handlers, and the `build_app` factory live here.
// `src/lib.rs` re-exports `build_app`; `src/main.rs` just binds the
// TCP listener and calls into the library.
//
// SECURITY: every request to /graphql must carry an `Authorization: Bearer
// <jwt>` header.  The JWT middleware (rate_limit::jwt_middleware) decodes and
// verifies the token, then injects TenantId + Claims into request extensions.
// The rate-limit middleware (rate_limit::rate_limit_middleware) consumes one
// per-tenant token and returns 429 on exhaustion.  Both middlewares run only
// on the /graphql route; /health is unauthenticated.

use anyhow::{Context as _, Result};
use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use feature_store::judicialpredict::data_plane::feature_store::v1::{
    feature_store_service_client::FeatureStoreServiceClient, GetFeatureRequest,
};
use sqlx::PgPool;
use std::sync::Arc;
use tonic::transport::Channel;
use uuid::Uuid;

use crate::auth::Claims;
use crate::rate_limit::{MemoryStore, RateLimitConfig, RateLimitStore};

// ---------------------------------------------------------------------------
// Tenant identity — injected by jwt_middleware; read by resolvers + rate-limit
// ---------------------------------------------------------------------------

/// Wraps the tenant UUID extracted from the validated JWT `tenant_id` claim.
#[derive(Clone, Copy)]
pub(crate) struct TenantId(pub(crate) Uuid);

// ---------------------------------------------------------------------------
// GraphQL data transfer objects
// ---------------------------------------------------------------------------

/// A feature as returned by the GraphQL API.
#[derive(SimpleObject)]
struct FeatureDto {
    /// Storage primary key (UUID, used as the stable feature_id in Sprint 1).
    id: String,
    /// Stable feature identifier, e.g. "judge.reversal_rate.circuit9".
    name: String,
    /// JSON-encoded feature value.
    value_json: String,
    /// Compliance tier: "TIER_A" | "TIER_B" | "TIER_C" | "TIER_D".
    tier: String,
    /// Sensitivity: "PUBLIC" | "QUASI_PUBLIC" | "INFERRED" | "PROTECTED".
    sensitivity: String,
    /// Case UUID, if the feature is case-scoped.
    case_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Tier/sensitivity i32 → string helpers
// ---------------------------------------------------------------------------

/// Map the proto wire integer back to the SQL tier enum string.
fn tier_to_str(i: i32) -> String {
    match i {
        1 => "TIER_A",
        2 => "TIER_B",
        3 => "TIER_C",
        4 => "TIER_D",
        _ => "TIER_UNSPECIFIED",
    }
    .to_string()
}

/// Map the proto wire integer back to the SQL sensitivity enum string.
fn sensitivity_to_str(i: i32) -> String {
    match i {
        1 => "PUBLIC",
        2 => "QUASI_PUBLIC",
        3 => "INFERRED",
        4 => "PROTECTED",
        _ => "SENSITIVITY_UNSPECIFIED",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// GraphQL schema — Query root
// ---------------------------------------------------------------------------

struct Query;

#[Object]
impl Query {
    /// Liveness check — always returns "ok".
    async fn healthcheck(&self) -> &str {
        "ok"
    }

    /// Look up a single feature by its storage UUID.
    ///
    /// Returns `null` if the feature does not exist or belongs to a
    /// different tenant (RLS enforces isolation transparently in feature-store).
    ///
    /// Returns a GraphQL error if the feature-store gRPC service is unavailable.
    ///
    /// Requires a valid `Authorization: Bearer <jwt>` header on the HTTP request.
    async fn feature(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<FeatureDto>> {
        let TenantId(tenant_id) = *ctx.data::<TenantId>().map_err(|_| "missing tenant id")?;

        // Clone the client — tonic clients are cheap to clone (shared channel).
        let mut client = ctx
            .data::<FeatureStoreServiceClient<Channel>>()
            .map_err(|_| "missing feature-store client")?
            .clone();

        // Attach tenant-id to the outbound gRPC request metadata.
        let mut request = tonic::Request::new(GetFeatureRequest {
            // Sprint 1: feature_id is the DB UUID.
            feature_id: id.clone(),
            case_id: String::new(),
            permitted_use: 0,
        });
        let tenant_val: tonic::metadata::MetadataValue<tonic::metadata::Ascii> = tenant_id
            .to_string()
            .parse()
            .map_err(|_| "invalid tenant id format")?;
        request.metadata_mut().insert("tenant-id", tenant_val);

        let response = client
            .get_feature(request)
            .await
            .map_err(|status| {
                async_graphql::Error::new(format!(
                    "feature-store unavailable: {} {}",
                    status.code(),
                    status.message()
                ))
            })?;

        let feature = response.into_inner().feature;
        Ok(feature.map(|f| FeatureDto {
            id: f.feature_id,
            name: f.name,
            value_json: f.value_json,
            tier: tier_to_str(f.tier),
            sensitivity: sensitivity_to_str(f.sensitivity),
            case_id: if f.case_id.is_empty() {
                None
            } else {
                Some(f.case_id)
            },
        }))
    }

    /// List the current tenant's cases, most-recent-first.
    /// Resolves `Query.listCases(limit: Int = 20, offset: Int = 0)` (S4.3 / JP-57).
    async fn list_cases(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] limit: i32,
        #[graphql(default = 0)] offset: i32,
    ) -> async_graphql::Result<crate::graphql_predict::CaseConnection> {
        use async_graphql::{Json, ID};
        use crate::graphql_predict::{Case, CaseConnection, PredictInput, PredictResult, RecommendationDto, compute_next_offset};
        use sqlx::Row as _;

        if !(1..=100).contains(&limit) {
            return Err(async_graphql::Error::new("limit must be between 1 and 100"));
        }
        if offset < 0 {
            return Err(async_graphql::Error::new("offset must be >= 0"));
        }

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

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

        sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
            .execute(&mut *tx)
            .await
            .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        let total_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cases WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| async_graphql::Error::new(format!("count query failed: {e}")))?;

        // S11.5 — pull date_filed and sort by COALESCE(date_filed, created_at)
        // so the dashboard displays operator-supplied filing dates when
        // present, ordering still consistent for legacy NULL rows.
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, input_features, prediction, recommendation,
                   created_by, created_at::text AS created_at_s,
                   date_filed::text AS date_filed_s
            FROM   cases
            WHERE  tenant_id = $1
            ORDER BY COALESCE(date_filed, created_at::date) DESC, created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("list query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx commit: {e}")))?;

        let mut nodes = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.try_get("id")
                .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
            let tenant_id_col: Uuid = row.try_get("tenant_id")
                .map_err(|e| async_graphql::Error::new(format!("row.tenant_id: {e}")))?;
            let created_by: Option<Uuid> = row.try_get("created_by")
                .map_err(|e| async_graphql::Error::new(format!("row.created_by: {e}")))?;
            let created_at: String = row.try_get("created_at_s")
                .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;
            let input_features_val: serde_json::Value = row.try_get("input_features")
                .map_err(|e| async_graphql::Error::new(format!("case {id}: input_features NULL: {e}")))?;
            let prediction_val: serde_json::Value = row.try_get("prediction")
                .map_err(|e| async_graphql::Error::new(format!("case {id}: prediction NULL: {e}")))?;
            let recommendation_val: serde_json::Value = row.try_get("recommendation")
                .map_err(|e| async_graphql::Error::new(format!("case {id}: recommendation NULL: {e}")))?;

            let input_features: PredictInput = serde_json::from_value(input_features_val)
                .map_err(|e| async_graphql::Error::new(format!("case {id}: input_features parse: {e}")))?;
            let prediction: PredictResult = serde_json::from_value(prediction_val)
                .map_err(|e| async_graphql::Error::new(format!("case {id}: prediction parse: {e}")))?;
            let recommendation: RecommendationDto = serde_json::from_value(recommendation_val)
                .map_err(|e| async_graphql::Error::new(format!("case {id}: recommendation parse: {e}")))?;

            let date_filed: Option<String> = row.try_get("date_filed_s")
                .map_err(|e| async_graphql::Error::new(format!("row.date_filed: {e}")))?;

            nodes.push(Case {
                id: ID::from(id.to_string()),
                tenant_id: ID::from(tenant_id_col.to_string()),
                input_features: Json(input_features),
                prediction,
                recommendation,
                created_by: created_by.map(|u| ID::from(u.to_string())),
                created_at,
                // S6.8 — listCases is a summary view; it does not fetch the
                // nlp_suggestion column.  Clients that need it use the
                // single-case `case(id)` query.
                nlp_suggestion: None,
                // S10.5 — same rationale: ideology_provenance is fetched
                // only by the single-case query for the compliance footer.
                ideology_provenance: None,
                // S11.5 — date_filed IS fetched here; powers the dashboard
                // "DATE FILED" column.
                date_filed,
            });
        }

        let next_offset = compute_next_offset(offset, nodes.len(), total_count);
        Ok(CaseConnection { nodes, total_count, next_offset })
    }

    /// Fetch one case by UUID; null if missing or other-tenant. Resolves `Query.case(id)` (S4.4 / JP-58).
    #[graphql(name = "case")]
    async fn get_case(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<crate::graphql_predict::Case>> {
        use async_graphql::Json;
        use crate::graphql_predict::{
            Case, ExtractedFeatures, PredictInput, PredictResult, RecommendationDto,
        };
        use sqlx::Row as _;

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let case_uuid = Uuid::parse_str(id.as_str())
            .map_err(|_| async_graphql::Error::new("invalid case id: must be a UUID v4"))?;

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

        sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
            .execute(&mut *tx)
            .await
            .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        let row_opt = sqlx::query(
            r#"
            SELECT id, tenant_id, input_features, prediction, recommendation,
                   created_by, created_at::text AS created_at_s, nlp_suggestion,
                   ideology_provenance, date_filed::text AS date_filed_s
            FROM   cases
            WHERE  id = $1 AND tenant_id = $2
            "#,
        )
        .bind(case_uuid)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("case query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("cases tx commit: {e}")))?;

        let Some(row) = row_opt else { return Ok(None); };

        let row_id: Uuid = row.try_get("id")
            .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
        let tenant_id_col: Uuid = row.try_get("tenant_id")
            .map_err(|e| async_graphql::Error::new(format!("row.tenant_id: {e}")))?;
        let created_by: Option<Uuid> = row.try_get("created_by")
            .map_err(|e| async_graphql::Error::new(format!("row.created_by: {e}")))?;
        let created_at: String = row.try_get("created_at_s")
            .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;

        let input_features_val: serde_json::Value = row.try_get("input_features")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: input_features NULL: {e}")))?;
        let prediction_val: serde_json::Value = row.try_get("prediction")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: prediction NULL: {e}")))?;
        let recommendation_val: serde_json::Value = row.try_get("recommendation")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: recommendation NULL: {e}")))?;

        let input_features: PredictInput = serde_json::from_value(input_features_val)
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: input_features parse: {e}")))?;
        let prediction: PredictResult = serde_json::from_value(prediction_val)
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: prediction parse: {e}")))?;
        let recommendation: RecommendationDto = serde_json::from_value(recommendation_val)
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: recommendation parse: {e}")))?;

        // S6.8 — nlp_suggestion is nullable: NULL for cases created without
        // an opinion_text payload.
        let nlp_suggestion_val: Option<serde_json::Value> = row.try_get("nlp_suggestion")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: nlp_suggestion: {e}")))?;
        let nlp_suggestion: Option<ExtractedFeatures> = nlp_suggestion_val
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: nlp_suggestion parse: {e}")))?;

        // S10.5 — read the ideology provenance snapshot too (nullable;
        // NULL for cases created before Sprint 10).
        let ideology_provenance_val: Option<serde_json::Value> = row.try_get("ideology_provenance")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: ideology_provenance: {e}")))?;

        // S11.5 — operator-supplied filing date.
        let date_filed: Option<String> = row.try_get("date_filed_s")
            .map_err(|e| async_graphql::Error::new(format!("case {row_id}: date_filed: {e}")))?;

        Ok(Some(Case {
            id: async_graphql::ID::from(row_id.to_string()),
            tenant_id: async_graphql::ID::from(tenant_id_col.to_string()),
            input_features: Json(input_features),
            prediction,
            recommendation,
            created_by: created_by.map(|u| async_graphql::ID::from(u.to_string())),
            created_at,
            nlp_suggestion: nlp_suggestion.map(Json),
            ideology_provenance: ideology_provenance_val.map(Json),
            date_filed,
        }))
    }

    /// Full prediction history for a case, most-recent-first. Resolves `Query.casePredictions(id)` (S4.7 / JP-61).
    #[graphql(name = "casePredictions")]
    async fn case_predictions(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Vec<crate::graphql_predict::PredictionHistoryEntry>> {
        use crate::graphql_predict::{PredictResult, PredictionHistoryEntry};
        use sqlx::Row as _;

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let case_uuid = Uuid::parse_str(id.as_str())
            .map_err(|_| async_graphql::Error::new("invalid case id: must be a UUID v4"))?;

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
            .map_err(|e| async_graphql::Error::new(format!("predictions tx begin: {e}")))?;

        sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
            .execute(&mut *tx)
            .await
            .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        let rows = sqlx::query(
            r#"
            SELECT id, prediction, model_version, created_at::text AS created_at_s
            FROM   predictions
            WHERE  case_id = $1 AND tenant_id = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(case_uuid)
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("predictions query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("predictions tx commit: {e}")))?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in &rows {
            let entry_id: Uuid = row.try_get("id")
                .map_err(|e| async_graphql::Error::new(format!("row.id: {e}")))?;
            let prediction_val: serde_json::Value = row.try_get("prediction")
                .map_err(|e| async_graphql::Error::new(format!("prediction {entry_id}: prediction NULL: {e}")))?;
            let model_version: String = row.try_get("model_version")
                .map_err(|e| async_graphql::Error::new(format!("row.model_version: {e}")))?;
            let created_at: String = row.try_get("created_at_s")
                .map_err(|e| async_graphql::Error::new(format!("row.created_at: {e}")))?;

            let prediction: PredictResult = serde_json::from_value(prediction_val)
                .map_err(|e| async_graphql::Error::new(format!("prediction {entry_id}: parse error: {e}")))?;

            entries.push(PredictionHistoryEntry {
                id: async_graphql::ID::from(entry_id.to_string()),
                prediction,
                model_version,
                created_at,
            });
        }

        Ok(entries)
    }

    /// Aggregate stats for the current tenant's cases — drives the /cases
    /// dashboard cards (total, recommendation breakdown, avg P(win), last 7d).
    /// All counts honor RLS via `SET LOCAL app.current_tenant_id` + explicit
    /// `tenant_id = $1` filter.
    async fn case_stats(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<CaseStats> {
        use sqlx::Row as _;

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

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
            .map_err(|e| async_graphql::Error::new(format!("stats tx begin: {e}")))?;

        sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
            .execute(&mut *tx)
            .await
            .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        // One round-trip: COUNT(*) plus per-recommendation counts plus avg
        // p_win plus rows created in the last 7 days.  recommendation->>'kind'
        // is the discriminator; prediction->>'p_win' is snake_case per the
        // serde JSON encoding of PredictResult (the GraphQL alias is pWin).
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*)::bigint                                                                AS total,
                COUNT(*) FILTER (WHERE recommendation->>'kind' = 'Settle')::bigint              AS settle,
                COUNT(*) FILTER (WHERE recommendation->>'kind' = 'Try')::bigint                 AS try_count,
                COUNT(*) FILTER (WHERE recommendation->>'kind' = 'Borderline')::bigint          AS borderline,
                AVG((prediction->>'p_win')::float8)                                             AS avg_pwin,
                COUNT(*) FILTER (WHERE created_at >= NOW() - INTERVAL '7 days')::bigint         AS last7
            FROM cases
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("stats query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("stats tx commit: {e}")))?;

        let total: i64 = row.try_get("total").unwrap_or(0);
        let settle: i64 = row.try_get("settle").unwrap_or(0);
        let try_count: i64 = row.try_get("try_count").unwrap_or(0);
        let borderline: i64 = row.try_get("borderline").unwrap_or(0);
        let avg_pwin: Option<f64> = row.try_get("avg_pwin").ok();
        let last7: i64 = row.try_get("last7").unwrap_or(0);

        Ok(CaseStats {
            total_count: total,
            settle_count: settle,
            try_count,
            borderline_count: borderline,
            avg_p_win: avg_pwin,
            last_seven_days_count: last7,
        })
    }

    /// S5.8 — Suggest intake-form prefills from a prior opinion's plain text.
    ///
    /// Thin wrapper over `graphql_predict::extract_features_from_text`, which
    /// runs the S5.7 NLP helpers (`classify_case_type`, `detect_outcome`,
    /// `extract_judge_names`) and resolves judge severity from `judges`.
    /// S6.8 — the `createCase` mutation calls the same helper so the prefill
    /// suggestion and the persisted suggestion cannot drift.
    ///
    /// Pure-read; no audit row.  Requires a valid JWT but no special role.
    ///
    /// Sprint-10: optional `asOfYear` — when supplied, the MQ branch
    /// resolves to the highest term `<= asOfYear` instead of the
    /// latest-snapshot. JCS / DIME are single-point and ignore the param.
    async fn extract_features(
        &self,
        ctx: &Context<'_>,
        text: String,
        #[graphql(name = "asOfYear")] as_of_year: Option<i32>,
    ) -> async_graphql::Result<crate::graphql_predict::ExtractedFeatures> {
        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("cases store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new("cases store not configured (DATABASE_URL missing)")
            })?;

        crate::graphql_predict::extract_features_from_text(pool, tenant_id, &text, as_of_year).await
    }

    /// S6.12 — operator-facing read of the `audit_log` table, most-recent-first.
    ///
    /// Mirrors the Django admin viewer (S4.9): same RLS contract, same column
    /// shape.  Used by the Next.js `/audit` page rendered for operators whose
    /// JWT carries `role: "admin"` (see `web/middleware.ts`).
    ///
    /// RLS / role enforcement
    /// ----------------------
    /// The audit_log table has an RLS policy keyed on `app.current_tenant_id`
    /// so tenant-scoped operators only ever see their own tenant's rows.  This
    /// resolver applies the same `SET LOCAL` pattern used by `listCases` /
    /// `caseStats` to make that filter fire.
    ///
    /// TODO(S6.12 follow-up): once the auth issuer adds a `role` claim to
    /// `Claims`, gate this resolver server-side too (deny unless `role` is
    /// `"admin"` or `"super"`).  Until then defense-in-depth lives in the
    /// Next.js middleware.  Tenant isolation is already enforced by RLS.
    async fn audit_events(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 25)] limit: i32,
        #[graphql(default = 0)] offset: i32,
    ) -> async_graphql::Result<AuditConnection> {
        use crate::graphql_predict::compute_next_offset;
        use sqlx::Row as _;

        if !(1..=100).contains(&limit) {
            return Err(async_graphql::Error::new("limit must be between 1 and 100"));
        }
        if offset < 0 {
            return Err(async_graphql::Error::new("offset must be >= 0"));
        }

        let TenantId(tenant_id) = *ctx
            .data::<TenantId>()
            .map_err(|_| async_graphql::Error::new("missing tenant id"))?;

        let pool = ctx
            .data::<Option<Arc<PgPool>>>()
            .map_err(|_| async_graphql::Error::new("audit store unavailable"))?
            .as_ref()
            .ok_or_else(|| {
                async_graphql::Error::new("audit store not configured (DATABASE_URL missing)")
            })?;

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| async_graphql::Error::new(format!("audit tx begin: {e}")))?;

        // Same RLS pattern listCases / caseStats use — the audit_log_select
        // policy is keyed on `app.current_tenant_id`.
        sqlx::query(&format!("SET LOCAL app.current_tenant_id = '{tenant_id}'"))
            .execute(&mut *tx)
            .await
            .map_err(|e| async_graphql::Error::new(format!("SET LOCAL failed: {e}")))?;

        // Tenant scoping is double-enforced (RLS + explicit `tenant_id = $1`)
        // mirroring the existing case queries.
        let total_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("audit count query failed: {e}")))?;

        let rows = sqlx::query(
            r#"
            SELECT id,
                   tenant_id,
                   subject_id,
                   table_name,
                   row_pk,
                   action,
                   reason_code,
                   ts::text AS ts_s,
                   latency_ms
            FROM   audit_log
            WHERE  tenant_id = $1
            ORDER BY ts DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| async_graphql::Error::new(format!("audit list query failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| async_graphql::Error::new(format!("audit tx commit: {e}")))?;

        let mut nodes = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: i64 = row
                .try_get("id")
                .map_err(|e| async_graphql::Error::new(format!("audit row.id: {e}")))?;
            let tenant: Option<Uuid> = row
                .try_get("tenant_id")
                .map_err(|e| async_graphql::Error::new(format!("audit row.tenant_id: {e}")))?;
            let subject_id: Option<String> = row
                .try_get("subject_id")
                .map_err(|e| async_graphql::Error::new(format!("audit row.subject_id: {e}")))?;
            let table_name: String = row
                .try_get("table_name")
                .map_err(|e| async_graphql::Error::new(format!("audit row.table_name: {e}")))?;
            let row_pk: Option<String> = row
                .try_get("row_pk")
                .map_err(|e| async_graphql::Error::new(format!("audit row.row_pk: {e}")))?;
            let action: String = row
                .try_get("action")
                .map_err(|e| async_graphql::Error::new(format!("audit row.action: {e}")))?;
            let reason_code: Option<String> = row
                .try_get("reason_code")
                .map_err(|e| async_graphql::Error::new(format!("audit row.reason_code: {e}")))?;
            let ts: String = row
                .try_get("ts_s")
                .map_err(|e| async_graphql::Error::new(format!("audit row.ts: {e}")))?;
            let latency_ms: Option<i32> = row
                .try_get("latency_ms")
                .map_err(|e| async_graphql::Error::new(format!("audit row.latency_ms: {e}")))?;

            // `target` is a human-friendly composite of table_name + row_pk —
            // the UI shows it as a single column so each row reads cleanly.
            let target = match &row_pk {
                Some(pk) if !pk.is_empty() => format!("{table_name}:{pk}"),
                _ => table_name.clone(),
            };

            nodes.push(AuditEventDto {
                id: id.to_string(),
                tenant_id: tenant.map(|u| u.to_string()),
                actor: subject_id,
                action,
                target,
                reason_code,
                ts,
                latency_ms,
            });
        }

        let next_offset = compute_next_offset(offset, nodes.len(), total_count);
        Ok(AuditConnection { nodes, total_count, next_offset })
    }
}

/// S6.12 — one row from `audit_log`, surfaced to the operator UI.
///
/// `tenantId` is exposed (super operators are able to inspect cross-tenant
/// rows in the Django admin; the gateway resolver tenant-filters today but the
/// field is kept on the wire to ease the super-role follow-up).
///
/// `target` is a composite of `table_name` + `row_pk` (e.g. `cases:abc-…`).
/// The raw columns are not split because the UI only shows one column.
#[derive(SimpleObject)]
pub(crate) struct AuditEventDto {
    /// Numeric primary key of the audit row (stringified for GraphQL ID safety).
    pub id: String,
    /// Tenant UUID the row was written under; `None` for tenant-agnostic events.
    pub tenant_id: Option<String>,
    /// Actor that triggered the event (operator UUID / email / service principal).
    pub actor: Option<String>,
    /// Fully-qualified action / RPC name (e.g. `case.create`, `predict_case_outcome`).
    pub action: String,
    /// Composite `table_name:row_pk`, or just `table_name` when the event is row-less.
    pub target: String,
    /// Stable outcome code (`ok` / `err` / `timeout` / `rate_limit`).
    pub reason_code: Option<String>,
    /// ISO-8601 UTC timestamp (Postgres `ts` column cast to text).
    pub ts: String,
    /// Round-trip latency in milliseconds; `None` when not applicable.
    pub latency_ms: Option<i32>,
}

/// S6.12 — paginated list of audit events for the current tenant.
///
/// Same shape as `CaseConnection` so the Next.js pagination helper works on
/// both surfaces unchanged.
#[derive(SimpleObject)]
pub(crate) struct AuditConnection {
    /// Audit events on the current page, ordered `ts DESC`.
    pub nodes: Vec<AuditEventDto>,
    /// Total audit row count visible to this caller.
    pub total_count: i64,
    /// Offset for the next page, or `None` on the final page.
    pub next_offset: Option<i32>,
}

/// Aggregate counters returned by `Query.caseStats`.
#[derive(SimpleObject)]
pub(crate) struct CaseStats {
    /// Total number of cases stored for this tenant.
    total_count: i64,
    /// Cases whose recommendation kind is `Settle`.
    settle_count: i64,
    /// Cases whose recommendation kind is `Try`.
    try_count: i64,
    /// Cases whose recommendation kind is `Borderline`.
    borderline_count: i64,
    /// Mean of `prediction.pWin` across all cases (0.0..1.0). `null` when there
    /// are no cases yet.
    avg_p_win: Option<f64>,
    /// Cases created in the last seven days.
    last_seven_days_count: i64,
}

// ---------------------------------------------------------------------------
// Application state — owns the GraphQL schema, JWT secret, and rate-limit store
// ---------------------------------------------------------------------------

type AppSchema = Schema<Query, crate::graphql_predict::Mutation, EmptySubscription>;

/// Shared application state injected into every axum handler via `State<Arc<AppState>>`.
pub(crate) struct AppState {
    #[allow(private_interfaces)]
    pub(crate) schema: AppSchema,
    /// HS256 secret bytes. In dev, read from `JWT_SECRET` env var or the test
    /// constant. In prod, injected from External Secrets Operator (Sprint 3+).
    pub(crate) jwt_secret: Vec<u8>,
    /// Per-tenant token-bucket store. In-memory for now; Redis-backed in prod.
    pub(crate) rate_store: Arc<dyn RateLimitStore>,
    /// S6.15 — Postgres pool used by the PAT auth backend AND by the
    /// `/v1/cases` REST handler.  `None` in environments without a cases
    /// store; PAT auth then unconditionally 401s and the REST endpoint
    /// 503s (which matches the GraphQL `createCase` "store not configured"
    /// behavior).
    pub(crate) cases_pool: Option<Arc<sqlx::PgPool>>,
    /// ML inference client shared with the GraphQL mutation; both auth
    /// paths land on `case_import::do_create_case`, which takes this by
    /// reference.
    pub(crate) ml_client: crate::graphql_predict::MlInferenceClient,
    /// Optional audit recorder; mirrors what the GraphQL schema injects.
    pub(crate) audit_recorder: Option<audit_recorder::AuditRecorder>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Serves the GraphQL endpoint.
///
/// JWT validation and rate-limiting are handled by upstream middleware layers.
/// By the time this handler is reached, `TenantId` and `Claims` are guaranteed
/// to be present in request extensions.
async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    Extension(tenant_id): Extension<TenantId>,
    Extension(claims): Extension<Claims>,
    req: GraphQLRequest,
) -> Result<GraphQLResponse, StatusCode> {
    let gql_req = req
        .into_inner()
        .data(tenant_id)
        .data(claims);

    Ok(state.schema.execute(gql_req).await.into())
}

/// Simple HTTP liveness probe — does not require authentication.
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// App builder — exported via lib.rs for use in tests
// ---------------------------------------------------------------------------

/// Build the axum `Router`, wiring the GraphQL schema, JWT middleware,
/// per-tenant rate-limit middleware, and health endpoint.
///
/// # Parameters
/// - `feature_store_grpc_url` — gRPC endpoint for the feature-store service.
/// - `jwt_secret` — raw bytes of the HS256 signing secret.
/// - `rate_config` — rate-limiting parameters (RPM caps per tenant).
///
/// # Middleware stack (applied to `/graphql` only)
/// ```text
/// jwt_middleware (outermost) → rate_limit_middleware → graphql_handler
/// ```
pub async fn build_app(
    feature_store_grpc_url: &str,
    ml_inference_url: &str,
    cases_pool: Option<std::sync::Arc<sqlx::PgPool>>,
    audit_recorder: Option<audit_recorder::AuditRecorder>,
    jwt_secret: Vec<u8>,
    rate_config: RateLimitConfig,
) -> Result<Router> {
    let channel = Channel::from_shared(feature_store_grpc_url.to_string())
        .expect("invalid feature-store URL")
        .connect_lazy();

    let fs_client = FeatureStoreServiceClient::new(channel);

    // S6.15: build the ML client once and share it between the GraphQL
    // schema (via `.data()`) and the REST handler (via `AppState.ml_client`).
    let ml_client = {
        use crate::ml_inference_proto::inference_service_client::InferenceServiceClient;
        let channel = tonic::transport::Channel::from_shared(ml_inference_url.to_string())
            .context("ml-inference URL invalid")?
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .connect_lazy();
        crate::graphql_predict::MlInferenceClient { inner: InferenceServiceClient::new(channel) }
    };

    // Audit finding (2026-05-17): GraphQL introspection should be disabled
    // in prod (OWASP A05).  Gate on JP_ENV / NODE_ENV — when neither says
    // "development" we treat the deployment as prod-grade.  Dev / CI keep
    // introspection on so GraphiQL + tooling still work.
    let is_dev_env = std::env::var("JP_ENV")
        .or_else(|_| std::env::var("NODE_ENV"))
        .map(|v| v.eq_ignore_ascii_case("development") || v.eq_ignore_ascii_case("dev"))
        .unwrap_or(true);

    let mut schema_builder = Schema::build(
        Query,
        crate::graphql_predict::Mutation,
        EmptySubscription,
    )
    // gRPC client for ml-inference-svc (S5.4 / JP-71).  connect_lazy lets the
    // gateway start even if ml-inference is briefly unreachable.
    .data(ml_client.clone())
    .data(cases_pool.clone())
    .data(audit_recorder.clone())
    .data(fs_client);

    if !is_dev_env {
        schema_builder = schema_builder.disable_introspection();
        tracing::info!("GraphQL introspection disabled (JP_ENV / NODE_ENV not in dev)");
    }
    let schema = schema_builder.finish();

    let rate_store: Arc<dyn RateLimitStore> =
        Arc::new(MemoryStore::new(rate_config.requests_per_min));

    let state = Arc::new(AppState {
        schema,
        jwt_secret,
        rate_store,
        cases_pool,
        ml_client,
        audit_recorder,
    });

    // Build the /graphql sub-router with both auth + rate-limit middlewares.
    // route_layer is applied in reverse declaration order (last = outermost):
    //   jwt_middleware   ← outermost (runs first; injects extensions; 401 on failure)
    //   rate_limit_middleware ← inner (runs second; 429 on exhaustion)
    let graphql_router = Router::new()
        .route("/graphql", post(graphql_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::rate_limit_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::jwt_middleware,
        ));

    // S6.15: public REST API.  Same auth+rate-limit stack as /graphql so
    // every PAT goes through the same checks as a JWT.
    let rest_router = Router::new()
        .route("/v1/cases", post(crate::rest_api::create_case_v1))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::rate_limit_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&state),
            crate::rate_limit::jwt_middleware,
        ));

    let app = Router::new()
        .route("/health", get(health_handler))
        .merge(graphql_router)
        .merge(rest_router)
        .with_state(state);

    Ok(app)
}
