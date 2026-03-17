use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;

const MAX_MATRIX_CELLS: usize = 2500;

#[derive(Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize)]
pub struct RouteRequest {
    pub origin: LatLon,
    pub destination: LatLon,
    pub profile: String,
}

#[derive(Deserialize)]
pub struct MatrixRequest {
    pub origins: Vec<LatLon>,
    pub destinations: Vec<LatLon>,
    pub profile: String,
}

fn validate_latlon(latlon: &LatLon) -> bool {
    latlon.lat >= -90.0 && latlon.lat <= 90.0 && latlon.lon >= -180.0 && latlon.lon <= 180.0
}

pub async fn route_handler(
    State(state): State<AppState>,
    Json(body): Json<RouteRequest>,
) -> impl IntoResponse {
    if body.profile.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "profile must not be empty"})),
        )
            .into_response();
    }

    if !validate_latlon(&body.origin) || !validate_latlon(&body.destination) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "lat must be in [-90, 90] and lon in [-180, 180]"})),
        )
            .into_response();
    }

    let router = match &state.router {
        Some(r) => r.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "routing service unavailable"})),
            )
                .into_response();
        }
    };

    match router
        .route(
            (body.origin.lat, body.origin.lon),
            (body.destination.lat, body.destination.lon),
            &body.profile,
        )
        .await
    {
        Ok(result) => match serde_json::to_value(result) {
            Ok(value) => (StatusCode::OK, Json(value)).into_response(),
            Err(e) => {
                tracing::error!(error = %e, "route serialization error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "route error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "routing failed"})),
            )
                .into_response()
        }
    }
}

pub async fn matrix_handler(
    State(state): State<AppState>,
    Json(body): Json<MatrixRequest>,
) -> impl IntoResponse {
    if body.profile.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "profile must not be empty"})),
        )
            .into_response();
    }

    if body.origins.is_empty() || body.destinations.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "origins and destinations must not be empty"})),
        )
            .into_response();
    }

    if body.origins.len() * body.destinations.len() > MAX_MATRIX_CELLS {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "matrix size exceeds maximum of 2500 cells"})),
        )
            .into_response();
    }

    let invalid_coord = body
        .origins
        .iter()
        .chain(body.destinations.iter())
        .any(|p| !validate_latlon(p));
    if invalid_coord {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "lat must be in [-90, 90] and lon in [-180, 180]"})),
        )
            .into_response();
    }

    let router = match &state.router {
        Some(r) => r.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "routing service unavailable"})),
            )
                .into_response();
        }
    };

    let origins: Vec<(f64, f64)> = body.origins.iter().map(|p| (p.lat, p.lon)).collect();
    let destinations: Vec<(f64, f64)> = body.destinations.iter().map(|p| (p.lat, p.lon)).collect();

    match router.matrix(&origins, &destinations, &body.profile).await {
        Ok(result) => match serde_json::to_value(result) {
            Ok(value) => (StatusCode::OK, Json(value)).into_response(),
            Err(e) => {
                tracing::error!(error = %e, "matrix serialization error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal server error"})),
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "matrix error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "matrix computation failed"})),
            )
                .into_response()
        }
    }
}
