use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;
use atlas_core::{GeocodeOpts, Lang, ReverseOpts};

#[derive(Deserialize)]
pub struct ForwardParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub country: Option<String>,
    pub lang: Option<String>,
}

#[derive(Deserialize)]
pub struct ReverseParams {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub limit: Option<usize>,
    pub lang: Option<String>,
}

fn parse_lang(s: &str) -> Option<Lang> {
    match s.to_lowercase().as_str() {
        "en" => Some(Lang::En),
        "fr" => Some(Lang::Fr),
        "ar" => Some(Lang::Ar),
        "sw" => Some(Lang::Sw),
        "tw" => Some(Lang::Tw),
        "yo" => Some(Lang::Yo),
        other => Some(Lang::Other(other.to_string())),
    }
}

pub async fn forward(
    State(state): State<AppState>,
    Query(params): Query<ForwardParams>,
) -> impl IntoResponse {
    let query = match &params.q {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing required parameter: q"})),
            )
                .into_response()
        }
    };

    if query.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "query too long"})),
        )
            .into_response();
    }

    let geocoder = match &state.geocoder {
        Some(g) => g,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "geocoding service unavailable"})),
            )
                .into_response()
        }
    };

    let opts = GeocodeOpts {
        limit: params.limit.unwrap_or(5).min(50),
        country: params.country.clone(),
        lang: params.lang.as_deref().and_then(parse_lang),
    };

    match geocoder.geocode(&query, &opts).await {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({"results": results})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "geocode error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}

pub async fn reverse(
    State(state): State<AppState>,
    Query(params): Query<ReverseParams>,
) -> impl IntoResponse {
    let (lat, lon) = match (params.lat, params.lon) {
        (Some(lat), Some(lon)) => {
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "coordinates out of range"})),
                )
                    .into_response();
            }
            (lat, lon)
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "missing required parameters: lat, lon"})),
            )
                .into_response()
        }
    };

    let geocoder = match &state.geocoder {
        Some(g) => g,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "geocoding service unavailable"})),
            )
                .into_response()
        }
    };

    let opts = ReverseOpts {
        limit: params.limit.unwrap_or(5).min(50),
        lang: params.lang.as_deref().and_then(parse_lang),
        ..Default::default()
    };

    match geocoder.reverse(lat, lon, &opts).await {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({"results": results})),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "reverse geocode error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
