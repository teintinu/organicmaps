//! HTTP layer implementing `plans/ev/api/hazard-api.v1.yaml` (v1 subset).

use crate::domain::{Hazard, HazardDraft, HazardType, Point};
use crate::store::{CreateOutcome, Device, HazardStore, ListQuery, MemoryStore, Vote};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn HazardStore>,
    pub pepper: Arc<String>,
    pub meta: Meta,
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(pepper: impl Into<String>) -> Self {
        Self::with_store(Arc::new(MemoryStore::new()), pepper)
    }

    pub fn with_store(store: Arc<dyn HazardStore>, pepper: impl Into<String>) -> Self {
        Self {
            store,
            pepper: Arc::new(pepper.into()),
            meta: Meta::default(),
            limiter: Arc::new(RateLimiter::default()),
        }
    }
}

/// Fixed-window limiter, per token, in process. A shared Redis limiter is the
/// multi-replica upgrade; this is enough for a single node.
#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (i64, u32)>>,
}

impl RateLimiter {
    /// Returns seconds to wait when the caller is over the limit.
    pub fn check(
        &self,
        key: &str,
        limit: u32,
        window_secs: i64,
        now: DateTime<Utc>,
    ) -> Result<(), i64> {
        let now_ms = now.timestamp_millis();
        let window_ms = window_secs * 1000;
        let mut windows = self.windows.lock().unwrap();
        let entry = windows.entry(key.to_string()).or_insert((now_ms, 0));
        if now_ms - entry.0 >= window_ms {
            *entry = (now_ms, 0);
        }
        if entry.1 >= limit {
            let retry_ms = window_ms - (now_ms - entry.0);
            return Err((retry_ms / 1000).max(1));
        }
        entry.1 += 1;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub name: String,
    pub operator: String,
    pub schema_version: String,
    pub license: String,
    pub attribution: String,
    /// minLon, minLat, maxLon, maxLat
    pub coverage: Vec<[f64; 4]>,
    pub capabilities: Capabilities,
    pub max_batch: usize,
    pub ttl_seconds: BTreeMap<String, i64>,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            name: "hazardd".into(),
            operator: "Organic Maps EV (reference instance)".into(),
            schema_version: "1.0".into(),
            license: "ODbL-1.0".into(),
            attribution: "Organic Maps contributors".into(),
            coverage: vec![[-74.0, -34.0, -34.0, 6.0]],
            capabilities: Capabilities {
                reports: true,
                votes: true,
                abuse: true,
                live_availability: false,
            },
            max_batch: 50,
            ttl_seconds: BTreeMap::from([
                ("accident".into(), HazardType::Accident.ttl().num_seconds()),
                ("pothole".into(), HazardType::Pothole.ttl().num_seconds()),
                (
                    "road_closure".into(),
                    HazardType::RoadClosure.ttl().num_seconds(),
                ),
            ]),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub reports: bool,
    pub votes: bool,
    pub abuse: bool,
    pub live_availability: bool,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    title: &'static str,
    detail: Option<String>,
    retry_after: Option<i64>,
}

impl ApiError {
    fn new(status: StatusCode, title: &'static str) -> Self {
        Self {
            status,
            title,
            detail: None,
            retry_after: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "missing or invalid device token")
    }

    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "hazard not found")
    }

    fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid request").with_detail(detail)
    }

    fn too_many(retry_after: i64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            title: "rate limited",
            detail: Some(format!("retry in {retry_after}s")),
            retry_after: Some(retry_after),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "type": "about:blank",
            "title": self.title,
            "status": self.status.as_u16(),
            "detail": self.detail,
        });
        let mut response = (self.status, Json(body)).into_response();
        if let Some(retry) = self.retry_after {
            if let Ok(value) = retry.to_string().parse() {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/v1/meta", get(meta))
        .route("/v1/devices", post(create_device))
        .route("/v1/me", delete(delete_me))
        .route("/v1/hazards", get(list_hazards).post(create_hazard))
        .route("/v1/hazards:batch", post(batch_hazards))
        .route("/v1/hazards/{id}", get(get_hazard))
        .route("/v1/hazards/{id}/votes", post(vote))
        .route("/v1/hazards/{id}/abuse", post(abuse))
        .fallback(not_found_handler)
        .with_state(state)
}

async fn not_found_handler() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "unknown endpoint")
}

async fn health(State(state): State<AppState>) -> Response {
    let ok = state.store.health().await;
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(json!({ "status": if ok { "ok" } else { "degraded" }, "version": env!("CARGO_PKG_VERSION") }))).into_response()
}

async fn meta(State(state): State<AppState>) -> Json<Meta> {
    Json(state.meta.clone())
}

fn token_hash(pepper: &str, token: &str) -> String {
    blake3::hash(format!("{pepper}:{token}").as_bytes())
        .to_hex()
        .to_string()
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

async fn require_device(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(Device, String), ApiError> {
    let token = bearer(headers).ok_or_else(ApiError::unauthorized)?;
    let hash = token_hash(&state.pepper, token);
    let device = state
        .store
        .device(&hash, Utc::now())
        .await
        .ok_or_else(ApiError::unauthorized)?;
    Ok((device, hash))
}

async fn create_device(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let hash = token_hash(&state.pepper, &token);
    let now = Utc::now();
    let device = state.store.create_device(hash, now).await;
    (
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "expiresAt": device.expires_at.to_rfc3339(),
            "scopes": ["report", "vote", "read"],
        })),
    )
}

async fn delete_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let token = bearer(&headers).ok_or_else(ApiError::unauthorized)?;
    let hash = token_hash(&state.pepper, token);
    state.store.remove_device(&hash).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ListParams {
    bbox: Option<String>,
    since: Option<String>,
    types: Option<String>,
    limit: Option<usize>,
}

fn parse_bbox(raw: &str) -> Result<(f64, f64, f64, f64), ApiError> {
    let parts: Vec<f64> = raw
        .split(',')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| ApiError::bad_request("bbox must be minLon,minLat,maxLon,maxLat"))?;
    if parts.len() != 4 {
        return Err(ApiError::bad_request("bbox must have 4 values"));
    }
    let (min_lon, min_lat, max_lon, max_lat) = (parts[0], parts[1], parts[2], parts[3]);
    if min_lon > max_lon || min_lat > max_lat {
        return Err(ApiError::bad_request("bbox min must be <= max"));
    }
    if max_lon - min_lon > 0.5 || max_lat - min_lat > 0.5 {
        return Err(ApiError::bad_request("bbox larger than 0.5 x 0.5 degrees"));
    }
    Ok((min_lon, min_lat, max_lon, max_lat))
}

fn parse_types(raw: &str) -> Vec<HazardType> {
    raw.split(',')
        .filter_map(|part| HazardType::parse(part.trim()))
        .collect()
}

#[derive(Serialize)]
struct CollectionMeta {
    #[serde(rename = "nextSince")]
    next_since: String,
    #[serde(rename = "serverTime")]
    server_time: String,
    truncated: bool,
}

#[derive(Serialize)]
struct FeatureCollection {
    #[serde(rename = "type")]
    kind: &'static str,
    features: Vec<Feature>,
    meta: CollectionMeta,
}

#[derive(Serialize)]
struct Feature {
    #[serde(rename = "type")]
    kind: &'static str,
    geometry: Point,
    properties: Hazard,
}

async fn list_hazards(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<FeatureCollection>, ApiError> {
    let now = Utc::now();
    let bbox = params.bbox.as_deref().map(parse_bbox).transpose()?;
    let since_ms = match params.since.as_deref() {
        Some(raw) => Some(
            raw.parse::<i64>()
                .map_err(|_| ApiError::bad_request("since must be an integer cursor"))?,
        ),
        None => None,
    };
    let types = params.types.as_deref().map(parse_types).unwrap_or_default();
    let limit = params.limit.unwrap_or(200);
    let query = ListQuery {
        bbox,
        since_ms,
        types,
        limit,
    };

    let hazards = state.store.list(&query, now).await;
    let next_since = hazards
        .iter()
        .map(|h| h.updated_at.timestamp_millis())
        .max()
        .or(since_ms)
        .unwrap_or(0);

    let features = hazards
        .into_iter()
        .map(|hazard| Feature {
            kind: "Feature",
            geometry: hazard.geom.clone(),
            properties: hazard,
        })
        .collect();

    Ok(Json(FeatureCollection {
        kind: "FeatureCollection",
        features,
        meta: CollectionMeta {
            next_since: next_since.to_string(),
            server_time: now.to_rfc3339(),
            truncated: false,
        },
    }))
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

async fn create_hazard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(draft): Json<HazardDraft>,
) -> Result<(StatusCode, Json<Hazard>), ApiError> {
    let (device, token_hash) = require_device(&headers, &state).await?;
    if !draft.geom.is_valid() {
        return Err(ApiError::bad_request("coordinates out of range"));
    }
    if let Err(retry) = state
        .limiter
        .check(&format!("report:{token_hash}"), 10, 3600, Utc::now())
    {
        return Err(ApiError::too_many(retry));
    }

    let key = idempotency_key(&headers);
    let outcome = state
        .store
        .create(draft, &device.id, key.as_deref(), Utc::now())
        .await;
    Ok(match outcome {
        CreateOutcome::Created(hazard) => (StatusCode::CREATED, Json(hazard)),
        CreateOutcome::Merged(hazard) => (StatusCode::CONFLICT, Json(hazard)),
    })
}

#[derive(Deserialize)]
struct BatchBody {
    reports: Vec<HazardDraft>,
}

#[derive(Serialize)]
struct BatchResult {
    index: usize,
    status: &'static str,
    hazard: Option<Hazard>,
    reason: Option<String>,
}

async fn batch_hazards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (device, token_hash) = require_device(&headers, &state).await?;
    if body.reports.len() > state.meta.max_batch {
        return Err(ApiError::bad_request("too many reports in batch"));
    }
    if let Err(retry) = state
        .limiter
        .check(&format!("report:{token_hash}"), 10, 3600, Utc::now())
    {
        return Err(ApiError::too_many(retry));
    }
    let batch_key = idempotency_key(&headers).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut results = Vec::with_capacity(body.reports.len());
    for (index, draft) in body.reports.into_iter().enumerate() {
        if !draft.geom.is_valid() {
            results.push(BatchResult {
                index,
                status: "rejected",
                hazard: None,
                reason: Some("coordinates out of range".into()),
            });
            continue;
        }
        let key = format!("{batch_key}:{index}");
        match state
            .store
            .create(draft, &device.id, Some(&key), Utc::now())
            .await
        {
            CreateOutcome::Created(hazard) => results.push(BatchResult {
                index,
                status: "created",
                hazard: Some(hazard),
                reason: None,
            }),
            CreateOutcome::Merged(hazard) => results.push(BatchResult {
                index,
                status: "merged",
                hazard: Some(hazard),
                reason: None,
            }),
        }
    }
    Ok(Json(json!({ "results": results })))
}

async fn get_hazard(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Hazard>, ApiError> {
    state
        .store
        .get(&id)
        .await
        .map(Json)
        .ok_or_else(ApiError::not_found)
}

#[derive(Debug, Deserialize)]
struct VoteBody {
    vote: String,
}

async fn vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<VoteBody>,
) -> Result<Json<Hazard>, ApiError> {
    let (_device, token_hash) = require_device(&headers, &state).await?;
    if let Err(retry) = state
        .limiter
        .check(&format!("vote:{token_hash}"), 60, 3600, Utc::now())
    {
        return Err(ApiError::too_many(retry));
    }
    let vote = match body.vote.as_str() {
        "confirm" => Vote::Confirm,
        "deny" => Vote::Deny,
        "resolved" => Vote::Resolved,
        other => return Err(ApiError::bad_request(format!("unknown vote: {other}"))),
    };
    state
        .store
        .vote(&id, vote, Utc::now())
        .await
        .map(Json)
        .ok_or_else(ApiError::not_found)
}

async fn abuse(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let _device = require_device(&headers, &state).await?;
    state.store.get(&id).await.ok_or_else(ApiError::not_found)?;
    Ok(StatusCode::NO_CONTENT)
}
