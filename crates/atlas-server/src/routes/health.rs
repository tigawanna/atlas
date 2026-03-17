use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

pub async fn health() -> impl IntoResponse {
    StatusCode::OK
}

pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    if state.ready.load(std::sync::atomic::Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
