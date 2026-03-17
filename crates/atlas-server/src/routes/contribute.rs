use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use atlas_core::contribution::{ContributionType, LatLon, RouteContribution};
use atlas_core::rfc3339_now;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ContributeRequest {
    pub route_origin: LatLonInput,
    pub route_destination: LatLonInput,
    pub profile: String,
    pub issue_type: String,
    pub description: Option<String>,
    pub suggested_waypoints: Option<Vec<LatLonInput>>,
}

#[derive(Deserialize)]
pub struct LatLonInput {
    pub lat: f64,
    pub lon: f64,
}

fn parse_issue_type(s: &str) -> Option<ContributionType> {
    match s {
        "wrong_turn" => Some(ContributionType::WrongTurn),
        "road_closed" => Some(ContributionType::RoadClosed),
        "better_route" => Some(ContributionType::BetterRoute),
        "roundabout_error" => Some(ContributionType::RoundaboutError),
        "missing_road" => Some(ContributionType::MissingRoad),
        "speed_wrong" => Some(ContributionType::SpeedWrong),
        "other" => Some(ContributionType::Other),
        _ => None,
    }
}

fn validate_lat_lon(lat: f64, lon: f64) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

pub async fn contribute_handler(
    State(state): State<AppState>,
    Json(body): Json<ContributeRequest>,
) -> impl IntoResponse {
    if !validate_lat_lon(body.route_origin.lat, body.route_origin.lon) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid route_origin coordinates"})),
        )
            .into_response();
    }

    if !validate_lat_lon(body.route_destination.lat, body.route_destination.lon) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid route_destination coordinates"})),
        )
            .into_response();
    }

    if body.profile.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "profile must not be empty"})),
        )
            .into_response();
    }

    let issue_type = match parse_issue_type(&body.issue_type) {
        Some(t) => t,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid issue_type"})),
            )
                .into_response();
        }
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = rfc3339_now();

    let suggested_waypoints = body.suggested_waypoints.map(|wps| {
        wps.into_iter()
            .map(|w| LatLon {
                lat: w.lat,
                lon: w.lon,
            })
            .collect()
    });

    let contribution = RouteContribution {
        id: id.clone(),
        route_origin: LatLon {
            lat: body.route_origin.lat,
            lon: body.route_origin.lon,
        },
        route_destination: LatLon {
            lat: body.route_destination.lat,
            lon: body.route_destination.lon,
        },
        profile: body.profile,
        issue_type,
        description: body.description,
        suggested_waypoints,
        created_at: now,
    };

    let store = match &state.contributions {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "contribution service unavailable"})),
            )
                .into_response();
        }
    };

    if let Err(e) = store.save(&contribution) {
        tracing::error!(error = %e, "failed to save contribution");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "failed to save contribution"})),
        )
            .into_response();
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": id, "status": "received"})),
    )
        .into_response()
}
