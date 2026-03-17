use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use atlas_core::{AtlasError, ContributionStore, TelemetryStore};
use atlas_geocode::Geocoder;
use atlas_route::RouteEngine;
use atlas_search::SearchEngine;
use atlas_server::config::{Config, TileSource};
use atlas_server::middleware;
use atlas_server::middleware::auth::AuthState;
use atlas_server::middleware::rate_limit::RateLimitState;
use atlas_server::routes;
use atlas_server::state::AppState;
use atlas_tiles::{CachedTileStore, LocalStore, S3Store};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "atlas_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    tracing::info!(?config, "starting atlas-server");

    let prometheus_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install metrics recorder");

    let ready = Arc::new(AtomicBool::new(false));

    let tile_store: Arc<dyn atlas_tiles::TileStore> = match config.tile_source {
        TileSource::Local => match LocalStore::open(Path::new(&config.tile_dir)).await {
            Ok(store) => Arc::new(store),
            Err(e) => {
                tracing::warn!(error = %e, "tile store not available, starting without tiles");
                Arc::new(atlas_tiles::store::local::LocalStore::empty())
            }
        },
        TileSource::S3 => {
            let bucket = config.s3_bucket.as_deref().ok_or_else(|| {
                AtlasError::ConfigError(
                    "ATLAS_S3_BUCKET must be set when using S3 tile source".to_string(),
                )
            })?;
            let aws_config = aws_config::from_env()
                .region(aws_sdk_s3::config::Region::new(config.s3_region.clone()))
                .load()
                .await;
            let s3_client = aws_sdk_s3::Client::new(&aws_config);
            let store = S3Store::open(s3_client, bucket, &config.s3_tile_keys).await?;
            Arc::new(store)
        }
    };

    let estimated_entries = config.cache_size_mb * 1024 * 1024 / (20 * 1024);
    let cached_store = Arc::new(CachedTileStore::new(tile_store, estimated_entries));

    let geocoder = match Geocoder::new(
        Path::new(&config.geocode_index_dir),
        Path::new(&config.landmark_path),
        Path::new(&config.places_path),
    ) {
        Ok(g) => {
            tracing::info!("geocoder loaded");
            Some(Arc::new(g))
        }
        Err(e) => {
            tracing::warn!(error = %e, "geocoder not available, starting without geocoding");
            None
        }
    };

    let osm_path = Path::new(&config.osm_dir);
    let osm_dir_opt: Option<&Path> = if osm_path.exists() {
        Some(osm_path)
    } else {
        None
    };
    let contributions_path = Path::new(&config.contributions_dir);
    let speed_data_path = Path::new(&config.speed_data_path);
    let router = match RouteEngine::load_with_speed_data(
        Path::new(&config.route_dir),
        osm_dir_opt,
        Some(contributions_path),
        Some(speed_data_path),
    ) {
        Ok(engine) => {
            tracing::info!(profiles = ?engine.available_profiles(), "route engine loaded");
            Some(Arc::new(engine))
        }
        Err(e) => {
            tracing::warn!(error = %e, "route engine not available, starting without routing");
            None
        }
    };

    let search = match SearchEngine::open(Path::new(&config.search_index_dir)) {
        Ok(engine) => {
            tracing::info!("search engine loaded");
            Some(Arc::new(engine))
        }
        Err(e) => {
            tracing::warn!(error = %e, "search engine not available, starting without search");
            None
        }
    };

    let contributions = Arc::new(ContributionStore::new(Path::new(&config.contributions_dir)));
    tracing::info!(dir = %config.contributions_dir, "contribution store initialized");

    let telemetry = Arc::new(TelemetryStore::new(Path::new(&config.telemetry_dir)));
    tracing::info!(dir = %config.telemetry_dir, "telemetry store initialized");

    let dynamodb_client = if config.auth_enabled {
        let aws_config = aws_config::from_env()
            .region(aws_sdk_dynamodb::config::Region::new(
                config.dynamodb_region.clone(),
            ))
            .load()
            .await;
        Some(aws_sdk_dynamodb::Client::new(&aws_config))
    } else {
        None
    };

    let auth_state = AuthState::new(
        config.auth_enabled,
        dynamodb_client,
        config.dynamodb_table.clone(),
    );

    let rate_limit_state = RateLimitState::new();

    let state = AppState {
        tiles: cached_store,
        geocoder,
        router,
        search,
        contributions: Some(contributions),
        telemetry: Some(telemetry),
        speed_data_path: Some(std::path::PathBuf::from(&config.speed_data_path)),
        public_url: config.public_url.clone(),
        ready: ready.clone(),
    };

    ready.store(true, std::sync::atomic::Ordering::Relaxed);

    let public_routes = Router::new()
        .route("/health", get(routes::health::health))
        .route("/ready", get(routes::health::ready))
        .route("/metrics", get(routes::metrics::metrics_handler))
        .layer(axum::Extension(prometheus_handle));

    let api_routes = Router::new()
        .route(
            "/v1/tiles/{tileset}/{z}/{x}/{y_fmt}",
            get(routes::tiles::get_tile),
        )
        .route(
            "/v1/tiles/{tileset}/tilejson.json",
            get(routes::tiles::get_tilejson),
        )
        .route("/v1/geocode", get(routes::geocode::forward))
        .route("/v1/reverse", get(routes::geocode::reverse))
        .route("/v1/route", post(routes::route::route_handler))
        .route("/v1/matrix", post(routes::route::matrix_handler))
        .route("/v1/search", get(routes::search::search_handler))
        .route(
            "/v1/contribute",
            post(routes::contribute::contribute_handler),
        )
        .route("/v1/telemetry/start", post(routes::telemetry::start_trip))
        .route(
            "/v1/telemetry/{trip_id}/update",
            post(routes::telemetry::update_trip),
        )
        .route(
            "/v1/telemetry/{trip_id}/end",
            post(routes::telemetry::end_trip),
        )
        .layer(axum::middleware::from_fn(
            middleware::rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::auth::auth_middleware))
        .layer(axum::Extension(auth_state))
        .layer(axum::Extension(rate_limit_state))
        .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024));

    let app = Router::new()
        .merge(public_routes)
        .merge(api_routes)
        .layer(axum::middleware::from_fn(
            middleware::metrics::metrics_middleware,
        ))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
