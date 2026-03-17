use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use atlas_core::telemetry::{TelemetryPoint, TripTelemetry};
use atlas_core::{is_valid_rfc3339, rfc3339_diff_secs, rfc3339_now};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct StartTripRequest {
    pub profile: String,
    pub origin: LatLonInput,
    pub destination: LatLonInput,
}

#[derive(Deserialize)]
pub struct LatLonInput {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Deserialize)]
pub struct UpdateTripRequest {
    pub waypoints: Vec<WaypointInput>,
}

#[derive(Deserialize)]
pub struct WaypointInput {
    pub lat: f64,
    pub lon: f64,
    pub timestamp: String,
    pub speed_kmh: Option<f64>,
    pub bearing: Option<f64>,
}

fn validate_lat_lon(lat: f64, lon: f64) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

pub async fn start_trip(
    State(state): State<AppState>,
    Json(body): Json<StartTripRequest>,
) -> impl IntoResponse {
    if body.profile.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "profile must not be empty"})),
        )
            .into_response();
    }

    if !validate_lat_lon(body.origin.lat, body.origin.lon) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid origin coordinates"})),
        )
            .into_response();
    }

    if !validate_lat_lon(body.destination.lat, body.destination.lon) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid destination coordinates"})),
        )
            .into_response();
    }

    let store = match &state.telemetry {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "telemetry service unavailable"})),
            )
                .into_response();
        }
    };

    let trip_id = uuid::Uuid::new_v4().to_string();
    let now = rfc3339_now();

    let trip = TripTelemetry {
        trip_id: trip_id.clone(),
        profile: body.profile,
        started_at: now,
        ended_at: None,
        waypoints: Vec::new(),
    };

    if let Err(e) = store.save_trip(&trip) {
        tracing::error!(error = %e, "failed to save trip");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to create trip"})),
        )
            .into_response();
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"trip_id": trip_id})),
    )
        .into_response()
}

pub async fn update_trip(
    State(state): State<AppState>,
    Path(trip_id): Path<String>,
    Json(body): Json<UpdateTripRequest>,
) -> impl IntoResponse {
    if body.waypoints.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "waypoints must not be empty"})),
        )
            .into_response();
    }

    for wp in &body.waypoints {
        if !validate_lat_lon(wp.lat, wp.lon) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid waypoint coordinates"})),
            )
                .into_response();
        }
        if !is_valid_rfc3339(&wp.timestamp) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid waypoint timestamp, expected RFC3339"})),
            )
                .into_response();
        }
    }

    let store = match &state.telemetry {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "telemetry service unavailable"})),
            )
                .into_response();
        }
    };

    let mut trip = match store.load_trip(&trip_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "trip not found"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load trip");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to load trip"})),
            )
                .into_response();
        }
    };

    if trip.ended_at.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "trip already ended"})),
        )
            .into_response();
    }

    let new_points: Vec<TelemetryPoint> = body
        .waypoints
        .into_iter()
        .map(|wp| TelemetryPoint {
            lat: wp.lat,
            lon: wp.lon,
            timestamp: wp.timestamp,
            speed_kmh: wp.speed_kmh,
            bearing: wp.bearing,
        })
        .collect();

    let point_count = new_points.len();
    trip.waypoints.extend(new_points);

    if let Err(e) = store.save_trip(&trip) {
        tracing::error!(error = %e, "failed to save trip update");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to save trip update"})),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "received", "points": point_count})),
    )
        .into_response()
}

pub async fn end_trip(
    State(state): State<AppState>,
    Path(trip_id): Path<String>,
) -> impl IntoResponse {
    let store = match &state.telemetry {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "telemetry service unavailable"})),
            )
                .into_response();
        }
    };

    let mut trip = match store.load_trip(&trip_id) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "trip not found"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load trip");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to load trip"})),
            )
                .into_response();
        }
    };

    if trip.ended_at.is_some() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "trip already ended"})),
        )
            .into_response();
    }

    let now = rfc3339_now();
    trip.ended_at = Some(now);

    let (duration_s, distance_m) = compute_trip_stats(&trip);

    if let Err(e) = store.save_trip(&trip) {
        tracing::error!(error = %e, "failed to save trip end");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to save trip end"})),
        )
            .into_response();
    }

    if let Some(router) = &state.router {
        let speed_path = state.speed_data_path.as_deref();
        if let Err(e) = router.ingest_trip(&trip, speed_path).await {
            tracing::warn!(error = %e, "failed to ingest trip into speed map");
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "completed",
            "duration_s": duration_s,
            "distance_m": distance_m
        })),
    )
        .into_response()
}

fn compute_trip_stats(trip: &TripTelemetry) -> (u64, u64) {
    if trip.waypoints.len() < 2 {
        return (0, 0);
    }

    let mut total_distance_m: f64 = 0.0;
    for i in 0..trip.waypoints.len() - 1 {
        let a = &trip.waypoints[i];
        let b = &trip.waypoints[i + 1];
        total_distance_m += atlas_core::geo_utils::haversine_distance(a.lat, a.lon, b.lat, b.lon);
    }

    let first_ts = &trip.waypoints[0].timestamp;
    let last_ts = &trip.waypoints[trip.waypoints.len() - 1].timestamp;
    let duration_s = rfc3339_diff_secs(first_ts, last_ts)
        .map(|secs| secs.max(0.0) as u64)
        .unwrap_or(0);

    (duration_s, total_distance_m as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_trip_stats_accepts_rfc3339_offsets() {
        let trip = TripTelemetry {
            trip_id: "trip-1".to_string(),
            profile: "car".to_string(),
            started_at: "2026-03-17T10:00:00+01:00".to_string(),
            ended_at: None,
            waypoints: vec![
                TelemetryPoint {
                    lat: 5.603,
                    lon: -0.187,
                    timestamp: "2026-03-17T10:00:00+01:00".to_string(),
                    speed_kmh: None,
                    bearing: None,
                },
                TelemetryPoint {
                    lat: 5.604,
                    lon: -0.188,
                    timestamp: "2026-03-17T10:00:30+01:00".to_string(),
                    speed_kmh: None,
                    bearing: None,
                },
            ],
        };

        let (duration_s, distance_m) = compute_trip_stats(&trip);
        assert_eq!(duration_s, 30);
        assert!(distance_m > 0);
    }
}
