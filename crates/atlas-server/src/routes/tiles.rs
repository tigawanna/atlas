use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;
use atlas_core::{AtlasError, TileCoord, TileFormat};

pub async fn get_tile(
    State(state): State<AppState>,
    Path((tileset, z, x, y_fmt)): Path<(String, u8, u32, String)>,
) -> impl IntoResponse {
    if tileset.contains("..")
        || tileset.contains('/')
        || tileset.contains('\\')
        || tileset.contains('\0')
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let (y_str, ext) = match y_fmt.rsplit_once('.') {
        Some(parts) => parts,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    let y: u32 = match y_str.parse() {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    if TileFormat::from_extension(ext).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let coord = match TileCoord::new(z, x, y) {
        Ok(c) => c,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    match state.tiles.get_tile(&tileset, coord).await {
        Ok(Some(tile)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, tile.format.content_type())],
            tile.data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(AtlasError::TileNotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(error = %e, tileset = %tileset, "tile store error");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_tilejson(
    State(state): State<AppState>,
    Path(tileset): Path<String>,
) -> impl IntoResponse {
    if tileset.contains("..")
        || tileset.contains('/')
        || tileset.contains('\\')
        || tileset.contains('\0')
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match state.tiles.get_tilejson(&tileset, &state.public_url).await {
        Ok(tilejson) => match serde_json::to_string(&tilejson) {
            Ok(body) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response(),
            Err(e) => {
                tracing::error!(error = %e, "tilejson serialization failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    [(header::CONTENT_TYPE, "application/json")],
                    r#"{"error":"internal server error"}"#.to_string(),
                )
                    .into_response()
            }
        },
        Err(_) => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"tileset not found"}"#.to_string(),
        )
            .into_response(),
    }
}
