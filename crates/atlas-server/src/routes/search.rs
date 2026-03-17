use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use atlas_search::SearchOpts;

use crate::state::AppState;

const MAX_SEARCH_LIMIT: usize = 50;

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub category: Option<String>,
    pub radius_km: Option<f64>,
    pub limit: Option<usize>,
    pub country: Option<String>,
}

pub async fn search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let has_query = params
        .q
        .as_deref()
        .map(|q| !q.trim().is_empty())
        .unwrap_or(false);
    let has_category = params.category.is_some();

    if !has_query && !has_category {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "at least one of 'q' or 'category' is required"})),
        )
            .into_response();
    }

    let search_engine = match &state.search {
        Some(engine) => engine.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "search service unavailable"})),
            )
                .into_response();
        }
    };

    let query_text = params.q.unwrap_or_default();

    if query_text.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "query too long"})),
        )
            .into_response();
    }
    let opts = SearchOpts {
        limit: params.limit.unwrap_or(10).min(MAX_SEARCH_LIMIT),
        lat: params.lat,
        lon: params.lon,
        radius_km: params.radius_km,
        category: params.category,
        country: params.country,
    };

    let result = tokio::task::spawn_blocking(move || search_engine.search(&query_text, &opts))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));

    match result {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({"results": results})),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(error = %err, "search error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal server error"})),
            )
                .into_response()
        }
    }
}
