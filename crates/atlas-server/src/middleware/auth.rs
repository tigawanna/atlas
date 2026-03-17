use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use dashmap::DashMap;
use serde::Serialize;

const CACHE_TTL: Duration = Duration::from_secs(300);
const API_KEY_TABLE_ATTR: &str = "api_key";
const OWNER_ATTR: &str = "owner";
const TIER_ATTR: &str = "tier";
const RATE_LIMIT_RPM_ATTR: &str = "rate_limit_rpm";
const ENABLED_ATTR: &str = "enabled";

#[derive(Clone, Debug)]
pub struct ApiKeyInfo {
    pub api_key: String,
    pub owner: String,
    pub tier: String,
    pub rate_limit_rpm: u32,
}

#[derive(Clone)]
pub struct AuthState {
    pub enabled: bool,
    pub client: Option<aws_sdk_dynamodb::Client>,
    pub table_name: String,
    pub cache: Arc<DashMap<String, (ApiKeyInfo, Instant)>>,
}

impl AuthState {
    pub fn new(
        enabled: bool,
        client: Option<aws_sdk_dynamodb::Client>,
        table_name: String,
    ) -> Self {
        Self {
            enabled,
            client,
            table_name,
            cache: Arc::new(DashMap::new()),
        }
    }

    pub fn disabled() -> Self {
        Self::new(false, None, String::new())
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

fn unauthorized() -> Response {
    let body = serde_json::to_string(&ErrorBody {
        error: "unauthorized",
    })
    .unwrap_or_default();
    (
        StatusCode::UNAUTHORIZED,
        [("content-type", "application/json")],
        body,
    )
        .into_response()
}

fn extract_api_key(req: &Request) -> Option<String> {
    if let Some(key) = req.headers().get("X-API-Key") {
        if let Ok(key_str) = key.to_str() {
            return Some(key_str.to_owned());
        }
    }

    let query = req.uri().query().unwrap_or("");
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("api_key=") {
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }

    None
}

async fn lookup_key_in_dynamodb(
    client: &aws_sdk_dynamodb::Client,
    table_name: &str,
    api_key: &str,
) -> Option<ApiKeyInfo> {
    use aws_sdk_dynamodb::types::AttributeValue;

    let result = client
        .get_item()
        .table_name(table_name)
        .key(API_KEY_TABLE_ATTR, AttributeValue::S(api_key.to_owned()))
        .send()
        .await
        .ok()?;

    let item = result.item?;

    let enabled = item
        .get(ENABLED_ATTR)
        .and_then(|v| v.as_bool().ok())
        .copied()
        .unwrap_or(false);

    if !enabled {
        return None;
    }

    let owner = item
        .get(OWNER_ATTR)
        .and_then(|v| v.as_s().ok())
        .cloned()
        .unwrap_or_default();

    let tier = item
        .get(TIER_ATTR)
        .and_then(|v| v.as_s().ok())
        .cloned()
        .unwrap_or_default();

    let rate_limit_rpm = item
        .get(RATE_LIMIT_RPM_ATTR)
        .and_then(|v| v.as_n().ok())
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(60);

    Some(ApiKeyInfo {
        api_key: api_key.to_owned(),
        owner,
        tier,
        rate_limit_rpm,
    })
}

pub async fn auth_middleware(
    Extension(auth_state): Extension<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    if !auth_state.enabled {
        return next.run(req).await;
    }

    let api_key = match extract_api_key(&req) {
        Some(k) => k,
        None => return unauthorized(),
    };

    if let Some(entry) = auth_state.cache.get(&api_key) {
        let (info, cached_at) = entry.value();
        if cached_at.elapsed() < CACHE_TTL {
            let info = info.clone();
            drop(entry);
            req.extensions_mut().insert(info);
            return next.run(req).await;
        }
    }

    let client = match &auth_state.client {
        Some(c) => c,
        None => return unauthorized(),
    };

    match lookup_key_in_dynamodb(client, &auth_state.table_name, &api_key).await {
        Some(info) => {
            auth_state
                .cache
                .insert(api_key, (info.clone(), Instant::now()));
            req.extensions_mut().insert(info);
            next.run(req).await
        }
        None => unauthorized(),
    }
}
