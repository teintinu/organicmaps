//! Contract-level tests: the behaviour the mobile client depends on.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    hazardd::app(hazardd::AppState::new("test-pepper"))
}

async fn call(app: axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(request).await.expect("request");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn json_request(method: &str, uri: &str, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body.to_string())).expect("request")
}

async fn device_token(app: axum::Router) -> String {
    let (status, body) = call(app, json_request("POST", "/v1/devices", json!({}), None)).await;
    assert_eq!(status, StatusCode::CREATED);
    body["token"].as_str().expect("token").to_string()
}

#[tokio::test]
async fn meta_describes_the_instance() {
    let (status, body) = call(
        test_app(),
        Request::builder()
            .uri("/v1/meta")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schemaVersion"], "1.0");
    assert_eq!(body["license"], "ODbL-1.0");
    assert!(body["coverage"].is_array());
}

#[tokio::test]
async fn reporting_requires_a_device_token() {
    let request = json_request(
        "POST",
        "/v1/hazards",
        json!({"type": "pothole", "geom": {"type": "Point", "coordinates": [-46.6, -23.5]}}),
        None,
    );
    let (status, body) = call(test_app(), request).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["status"], 401);
}

#[tokio::test]
async fn creates_reads_and_lists_a_hazard() {
    let app = test_app();
    let token = device_token(app.clone()).await;

    let report = json!({
        "type": "pothole",
        "geom": {"type": "Point", "coordinates": [-46.6333, -23.5505]},
        "attrs": {"severity": "medium"}
    });
    let mut request = json_request("POST", "/v1/hazards", report, Some(&token));
    request
        .headers_mut()
        .insert("idempotency-key", "k-1".parse().unwrap());
    let (status, hazard) = call(app.clone(), request).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(hazard["type"], "pothole");
    assert_eq!(hazard["status"], "active");
    let id = hazard["id"].as_str().expect("id").to_string();

    let (status, fetched) = call(
        app.clone(),
        Request::builder()
            .uri(format!("/v1/hazards/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["id"], id);

    let (status, collection) = call(
        app,
        Request::builder()
            .uri("/v1/hazards?bbox=-46.7,-23.6,-46.5,-23.4")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(collection["type"], "FeatureCollection");
    assert_eq!(collection["features"].as_array().unwrap().len(), 1);
    assert!(collection["meta"]["nextSince"].is_string());
}

#[tokio::test]
async fn duplicate_report_merges_and_confirms() {
    let app = test_app();
    let token = device_token(app.clone()).await;
    let report =
        json!({"type": "debris", "geom": {"type": "Point", "coordinates": [-46.6, -23.5]}});

    let (first_status, _) = call(
        app.clone(),
        json_request("POST", "/v1/hazards", report.clone(), Some(&token)),
    )
    .await;
    let (second_status, merged) = call(
        app,
        json_request("POST", "/v1/hazards", report, Some(&token)),
    )
    .await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(merged["confirmations"], 1);
}

#[tokio::test]
async fn votes_update_confidence() {
    let app = test_app();
    let token = device_token(app.clone()).await;
    let report =
        json!({"type": "accident", "geom": {"type": "Point", "coordinates": [-46.6, -23.5]}});
    let (_, hazard) = call(
        app.clone(),
        json_request("POST", "/v1/hazards", report, Some(&token)),
    )
    .await;
    let id = hazard["id"].as_str().unwrap();

    let (status, voted) = call(
        app,
        json_request(
            "POST",
            &format!("/v1/hazards/{id}/votes"),
            json!({"vote": "confirm"}),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(voted["confirmations"], 1);
    assert!(voted["confidence"].as_f64().unwrap() > 0.6);
}

#[tokio::test]
async fn deleting_the_device_anonymizes_reports() {
    let app = test_app();
    let token = device_token(app.clone()).await;
    let report =
        json!({"type": "flooding", "geom": {"type": "Point", "coordinates": [-46.6, -23.5]}});
    let (_, hazard) = call(
        app.clone(),
        json_request("POST", "/v1/hazards", report, Some(&token)),
    )
    .await;
    let id = hazard["id"].as_str().unwrap().to_string();

    let (status, _) = call(
        app.clone(),
        Request::builder()
            .method("DELETE")
            .uri("/v1/me")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, fetched) = call(
        app,
        Request::builder()
            .uri(format!("/v1/hazards/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!fetched.as_object().unwrap().contains_key("reported_by"));
}

#[tokio::test]
async fn rejects_oversized_bbox() {
    let (status, body) = call(
        test_app(),
        Request::builder()
            .uri("/v1/hazards?bbox=-80,-40,-30,10")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["status"], 400);
}
