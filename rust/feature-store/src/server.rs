// JudicialPredict feature-store — gRPC server implementation.
//
// Implements FeatureStoreService from protos/judicialpredict/data_plane/feature_store/v1/.
// Each RPC:
//   1. Extracts `tenant-id` gRPC metadata → parses as UUID.
//   2. Sets the Postgres RLS tenant context via set_tenant_context.
//   3. Loads per-tenant overrides from OverridesCache (60-s TTL; DB fallback).
//   4. Delegates to repo functions; applies override enforcement before returning.
//
// Override enforcement (S2.12):
//   - disabled_features: any matching feature name → PERMISSION_DENIED.
//   - tier_overrides → TIER_C: downgraded feature → PERMISSION_DENIED.
//   GetFeature returns PERMISSION_DENIED for a blocked feature.
//   ListFeatures emits a PERMISSION_DENIED stream item and terminates for a
//   blocked feature (not a silent drop per the spec).

use audit_recorder::AuditRecorder;
use sqlx::PgPool;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::judicialpredict::data_plane::feature_store::v1::{
    feature_store_service_server::FeatureStoreService, Feature, GetFeatureRequest,
    GetFeatureResponse, IngestFeatureRequest, IngestFeatureResponse, ListFeaturesRequest,
    ListFeaturesResponse,
};
use crate::tenant_settings::{self, OverridesCache};

// ---------------------------------------------------------------------------
// Server struct
// ---------------------------------------------------------------------------

/// gRPC server for FeatureStoreService.
///
/// Wraps a PgPool that connects as `jp_app` (non-superuser, FORCE RLS).
/// Every request must supply a `tenant-id` metadata header containing a valid UUID.
pub struct FeatureStoreServer {
    pool: PgPool,
    /// 60-second in-process cache of per-tenant feature-tier overrides.
    overrides_cache: OverridesCache,
    /// Audit recorder used by the admin update_overrides path.
    pub recorder: AuditRecorder,
}

impl FeatureStoreServer {
    pub fn new(pool: PgPool, overrides_cache: OverridesCache, recorder: AuditRecorder) -> Self {
        Self {
            pool,
            overrides_cache,
            recorder,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract and parse the `tenant-id` gRPC metadata header as a UUID.
#[allow(clippy::result_large_err)]
fn extract_tenant_id<T>(req: &Request<T>) -> Result<Uuid, Status> {
    let val = req
        .metadata()
        .get("tenant-id")
        .ok_or_else(|| Status::unauthenticated("missing tenant-id metadata"))?
        .to_str()
        .map_err(|_| Status::invalid_argument("tenant-id metadata is not valid UTF-8"))?;
    Uuid::parse_str(val)
        .map_err(|_| Status::invalid_argument("tenant-id is not a valid UUID"))
}

/// Map a SQL tier string ("TIER_A", …) to the proto wire integer.
fn tier_str_to_i32(tier: &str) -> i32 {
    match tier {
        "TIER_A" => 1,
        "TIER_B" => 2,
        "TIER_C" => 3,
        "TIER_D" => 4,
        _ => 0,
    }
}

/// Map a SQL sensitivity string ("PUBLIC", …) to the proto wire integer.
fn sensitivity_str_to_i32(s: &str) -> i32 {
    match s {
        "PUBLIC" => 1,
        "QUASI_PUBLIC" => 2,
        "INFERRED" => 3,
        "PROTECTED" => 4,
        _ => 0,
    }
}

/// Convert a FeatureRow to the proto Feature message.
fn row_to_proto(r: crate::FeatureRow) -> Feature {
    Feature {
        // Use the DB UUID as the stable feature_id for now (Sprint 2 will add a
        // proper string-key lookup when the feature-key table lands).
        feature_id: r.id.to_string(),
        name: r.name,
        value_json: r.value.to_string(),
        tier: tier_str_to_i32(&r.tier),
        sensitivity: sensitivity_str_to_i32(&r.sensitivity),
        case_id: r.case_id.map(|id| id.to_string()).unwrap_or_default(),
        derived_at_unix: 0, // populated when derivation timestamps land in Sprint 2
    }
}

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

#[tonic::async_trait]
impl FeatureStoreService for FeatureStoreServer {
    /// Retrieve a single feature by its ID (currently the DB UUID).
    ///
    /// Returns PERMISSION_DENIED if the feature name is blocked by the tenant's
    /// override policy (disabled_features or tier_overrides → TIER_C).
    async fn get_feature(
        &self,
        request: Request<GetFeatureRequest>,
    ) -> Result<Response<GetFeatureResponse>, Status> {
        let tenant_id = extract_tenant_id(&request)?;
        let req = request.into_inner();

        // For Sprint 1 we treat feature_id as the DB UUID primary key.
        let feature_id = Uuid::parse_str(&req.feature_id)
            .map_err(|_| Status::invalid_argument("feature_id is not a valid UUID"))?;

        let mut tx = crate::set_tenant_context(&self.pool, tenant_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let row = crate::get_feature(&mut tx, feature_id, tenant_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // S2.12: enforce per-tenant overrides after the DB fetch.
        if let Some(ref r) = row {
            let overrides =
                tenant_settings::get_overrides(&self.pool, tenant_id, &self.overrides_cache)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
            if let Some(reason) =
                tenant_settings::check_feature_allowed(&overrides, &r.name)
            {
                return Err(Status::permission_denied(reason));
            }
        }

        let feature = row.map(row_to_proto);
        Ok(Response::new(GetFeatureResponse { feature }))
    }

    // Server-streaming response type generated by tonic.
    type ListFeaturesStream =
        tokio_stream::wrappers::ReceiverStream<Result<ListFeaturesResponse, Status>>;

    /// Stream all features for a case, respecting tenant RLS isolation and
    /// per-tenant overrides.
    ///
    /// For each blocked feature, a PERMISSION_DENIED stream item is sent
    /// (not a silent drop — per spec §3 override semantics).  The stream
    /// terminates after the first error item.
    async fn list_features(
        &self,
        request: Request<ListFeaturesRequest>,
    ) -> Result<Response<Self::ListFeaturesStream>, Status> {
        let tenant_id = extract_tenant_id(&request)?;
        let req = request.into_inner();

        let case_id = Uuid::parse_str(&req.case_id)
            .map_err(|_| Status::invalid_argument("case_id is not a valid UUID"))?;

        let mut tx = crate::set_tenant_context(&self.pool, tenant_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let rows = crate::list_features_for_case(&mut tx, case_id, tenant_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // S2.12: load overrides once for the entire batch.
        let overrides =
            tenant_settings::get_overrides(&self.pool, tenant_id, &self.overrides_cache)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

        // Buffer rows into an mpsc channel; blocked features emit a stream error.
        let cap = rows.len().max(1);
        let (tx_chan, rx) = tokio::sync::mpsc::channel(cap);
        for row in rows {
            // Check override policy before sending.
            if let Some(reason) = tenant_settings::check_feature_allowed(&overrides, &row.name) {
                // Send a PERMISSION_DENIED item; client will abort the stream.
                let _ = tx_chan
                    .send(Err(Status::permission_denied(reason)))
                    .await;
                break;
            }
            let msg = Ok(ListFeaturesResponse {
                feature: Some(row_to_proto(row)),
            });
            // Receiver gone early means the client cancelled — just stop.
            if tx_chan.send(msg).await.is_err() {
                break;
            }
        }

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    /// Ingest a derived feature into the feature store.
    async fn ingest_feature(
        &self,
        request: Request<IngestFeatureRequest>,
    ) -> Result<Response<IngestFeatureResponse>, Status> {
        let tenant_id = extract_tenant_id(&request)?;
        let req = request.into_inner();

        let f = req
            .feature
            .ok_or_else(|| Status::invalid_argument("feature field is required"))?;

        let payload = crate::IngestPayload {
            tenant_id,
            case_id: if f.case_id.is_empty() {
                None
            } else {
                Some(
                    Uuid::parse_str(&f.case_id)
                        .map_err(|_| Status::invalid_argument("case_id is not a valid UUID"))?,
                )
            },
            name: f.name,
            value: serde_json::from_str(&f.value_json)
                .map_err(|_| Status::invalid_argument("value_json is not valid JSON"))?,
            // Map proto i32 wire values back to SQL enum strings.
            tier: match f.tier {
                1 => "TIER_A".to_string(),
                2 => "TIER_B".to_string(),
                3 => "TIER_C".to_string(),
                4 => "TIER_D".to_string(),
                _ => return Err(Status::invalid_argument("invalid tier value")),
            },
            sensitivity: match f.sensitivity {
                1 => "PUBLIC".to_string(),
                2 => "QUASI_PUBLIC".to_string(),
                3 => "INFERRED".to_string(),
                4 => "PROTECTED".to_string(),
                _ => return Err(Status::invalid_argument("invalid sensitivity value")),
            },
            source: "grpc-ingest".to_string(), // Sprint 2: add source field to IngestFeatureRequest proto
            lineage: serde_json::json!({}),
        };

        let mut tx = crate::set_tenant_context(&self.pool, tenant_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let id = crate::ingest_feature(&mut tx, payload)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(IngestFeatureResponse {
            storage_id: id.to_string(),
            stored_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            idempotency_key: req.idempotency_key,
        }))
    }
}
