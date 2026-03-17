use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use atlas_core::{AtlasError, TelemetryStore, TileCoord, TileFormat};
use atlas_tiles::{TileResponse, TileStore};
use bytes::Bytes;

struct MockTileStore;

#[async_trait]
impl TileStore for MockTileStore {
    async fn get_tile(
        &self,
        tileset: &str,
        coord: TileCoord,
    ) -> Result<Option<TileResponse>, AtlasError> {
        if tileset == "test" && coord.z <= 14 {
            Ok(Some(TileResponse {
                data: Bytes::from_static(b"\x1a\x00"),
                format: TileFormat::Mvt,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_tilejson(
        &self,
        tileset: &str,
        public_url: &str,
    ) -> Result<tilejson::TileJSON, AtlasError> {
        if tileset == "test" {
            Ok(tilejson::tilejson! {
                tiles: vec![format!("{}/v1/tiles/test/{{z}}/{{x}}/{{y}}.mvt", public_url)],
            })
        } else {
            Err(AtlasError::TileNotFound)
        }
    }

    fn tilesets(&self) -> Vec<String> {
        vec!["test".to_string()]
    }
}

fn prometheus_handle() -> metrics_exporter_prometheus::PrometheusHandle {
    use std::sync::OnceLock;
    static HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> = OnceLock::new();
    HANDLE
        .get_or_init(|| {
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .install_recorder()
                .unwrap()
        })
        .clone()
}

fn build_test_app() -> axum::Router {
    use atlas_server::middleware::auth::AuthState;
    use atlas_server::middleware::rate_limit::RateLimitState;
    use axum::routing::{get, post};
    use axum::Router;

    let prometheus_handle = prometheus_handle();

    let state = atlas_server::state::AppState {
        tiles: Arc::new(MockTileStore),
        geocoder: None,
        router: None,
        search: None,
        contributions: None,
        telemetry: None,
        speed_data_path: None,
        public_url: "http://localhost:3001".to_string(),
        ready: Arc::new(AtomicBool::new(true)),
    };

    let auth_state = AuthState::disabled();
    let rate_limit_state = RateLimitState::new();

    Router::new()
        .route(
            "/v1/tiles/{tileset}/{z}/{x}/{y_fmt}",
            get(atlas_server::routes::tiles::get_tile),
        )
        .route(
            "/v1/tiles/{tileset}/tilejson.json",
            get(atlas_server::routes::tiles::get_tilejson),
        )
        .route("/v1/geocode", get(atlas_server::routes::geocode::forward))
        .route("/v1/reverse", get(atlas_server::routes::geocode::reverse))
        .route(
            "/v1/route",
            post(atlas_server::routes::route::route_handler),
        )
        .route(
            "/v1/matrix",
            post(atlas_server::routes::route::matrix_handler),
        )
        .route(
            "/v1/search",
            get(atlas_server::routes::search::search_handler),
        )
        .route(
            "/metrics",
            get(atlas_server::routes::metrics::metrics_handler),
        )
        .route("/health", get(atlas_server::routes::health::health))
        .route("/ready", get(atlas_server::routes::health::ready))
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::auth::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::metrics::metrics_middleware,
        ))
        .layer(axum::Extension(prometheus_handle))
        .layer(axum::Extension(auth_state))
        .layer(axum::Extension(rate_limit_state))
        .with_state(state)
}

#[tokio::test]
async fn health_returns_200() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn ready_returns_200_when_loaded() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/ready")).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn tile_returns_mvt_content_type() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/tiles/test/0/0/0.mvt"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/vnd.mapbox-vector-tile"
    );
}

#[tokio::test]
async fn missing_tileset_returns_404() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/tiles/nonexistent/0/0/0.mvt"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn invalid_coords_return_400() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/tiles/test/0/5/0.mvt"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn tilejson_returns_valid_json() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/tiles/test/tilejson.json"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("tiles").is_some());
}

#[tokio::test]
async fn geocode_without_service_returns_503() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/geocode?q=Makola"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn reverse_without_service_returns_503() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/reverse?lat=5.55&lon=-0.21"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn geocode_missing_query_returns_400() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/geocode"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn reverse_missing_coords_returns_400() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/reverse"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn route_without_service_returns_503() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body = serde_json::json!({
        "origin": {"lat": 5.55, "lon": -0.21},
        "destination": {"lat": 5.60, "lon": -0.18},
        "profile": "car"
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/route"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn matrix_without_service_returns_503() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body = serde_json::json!({
        "origins": [{"lat": 5.55, "lon": -0.21}],
        "destinations": [{"lat": 5.60, "lon": -0.18}],
        "profile": "car"
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/matrix"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn route_missing_fields_returns_400() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body = serde_json::json!({
        "origin": {"lat": 5.55, "lon": -0.21}
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/route"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 422);
}

#[tokio::test]
async fn search_without_service_returns_503() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/search?q=test"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn search_missing_params_returns_400() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/v1/search"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn metrics_returns_prometheus_format() {
    let app = build_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let resp = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(resp.status(), 200);

    let resp = reqwest::get(format!("http://{addr}/metrics"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("atlas_http"),
        "expected prometheus metrics, got: {body}"
    );
}

fn build_telemetry_test_app() -> axum::Router {
    use atlas_server::middleware::auth::AuthState;
    use atlas_server::middleware::rate_limit::RateLimitState;
    use axum::routing::post;
    use axum::Router;

    let telemetry_dir =
        std::env::temp_dir().join(format!("atlas-telemetry-integ-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&telemetry_dir);

    let state = atlas_server::state::AppState {
        tiles: Arc::new(MockTileStore),
        geocoder: None,
        router: None,
        search: None,
        contributions: None,
        telemetry: Some(Arc::new(TelemetryStore::new(&telemetry_dir))),
        speed_data_path: None,
        public_url: "http://localhost:3001".to_string(),
        ready: Arc::new(AtomicBool::new(true)),
    };

    let auth_state = AuthState::disabled();
    let rate_limit_state = RateLimitState::new();

    Router::new()
        .route(
            "/v1/telemetry/start",
            post(atlas_server::routes::telemetry::start_trip),
        )
        .route(
            "/v1/telemetry/{trip_id}/update",
            post(atlas_server::routes::telemetry::update_trip),
        )
        .route(
            "/v1/telemetry/{trip_id}/end",
            post(atlas_server::routes::telemetry::end_trip),
        )
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::auth::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(
            atlas_server::middleware::metrics::metrics_middleware,
        ))
        .layer(axum::Extension(auth_state))
        .layer(axum::Extension(rate_limit_state))
        .with_state(state)
}

#[tokio::test]
async fn telemetry_start_returns_trip_id() {
    let app = build_telemetry_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let body = serde_json::json!({
        "profile": "car",
        "origin": {"lat": 5.603, "lon": -0.187},
        "destination": {"lat": 5.55, "lon": -0.21}
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/telemetry/start"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json.get("trip_id").is_some());
    let trip_id = json["trip_id"].as_str().unwrap();
    assert!(!trip_id.is_empty());
}

#[tokio::test]
async fn telemetry_update_accepts_waypoints() {
    let app = build_telemetry_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();

    let start_body = serde_json::json!({
        "profile": "car",
        "origin": {"lat": 5.603, "lon": -0.187},
        "destination": {"lat": 5.55, "lon": -0.21}
    });
    let start_resp = client
        .post(format!("http://{addr}/v1/telemetry/start"))
        .json(&start_body)
        .send()
        .await
        .unwrap();
    let start_json: serde_json::Value = start_resp.json().await.unwrap();
    let trip_id = start_json["trip_id"].as_str().unwrap();

    let update_body = serde_json::json!({
        "waypoints": [
            {"lat": 5.60, "lon": -0.19, "timestamp": "2026-03-17T10:00:00Z", "speed_kmh": 35.2, "bearing": 180.0},
            {"lat": 5.59, "lon": -0.19, "timestamp": "2026-03-17T10:00:05Z", "speed_kmh": 40.1, "bearing": 175.0}
        ]
    });

    let resp = client
        .post(format!("http://{addr}/v1/telemetry/{trip_id}/update"))
        .json(&update_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["status"], "received");
    assert_eq!(json["points"], 2);
}

#[tokio::test]
async fn telemetry_end_trip_completes() {
    let app = build_telemetry_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();

    let start_body = serde_json::json!({
        "profile": "car",
        "origin": {"lat": 5.603, "lon": -0.187},
        "destination": {"lat": 5.55, "lon": -0.21}
    });
    let start_resp = client
        .post(format!("http://{addr}/v1/telemetry/start"))
        .json(&start_body)
        .send()
        .await
        .unwrap();
    let start_json: serde_json::Value = start_resp.json().await.unwrap();
    let trip_id = start_json["trip_id"].as_str().unwrap();

    let resp = client
        .post(format!("http://{addr}/v1/telemetry/{trip_id}/end"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["status"], "completed");
    assert!(json.get("duration_s").is_some());
    assert!(json.get("distance_m").is_some());
}

#[tokio::test]
async fn telemetry_update_nonexistent_trip_returns_404() {
    let app = build_telemetry_test_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();
    let update_body = serde_json::json!({
        "waypoints": [
            {"lat": 5.60, "lon": -0.19, "timestamp": "2026-03-17T10:00:00Z"}
        ]
    });

    let resp = client
        .post(format!("http://{addr}/v1/telemetry/nonexistent-id/update"))
        .json(&update_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
