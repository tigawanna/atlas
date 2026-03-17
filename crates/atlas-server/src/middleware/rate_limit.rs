use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use dashmap::DashMap;
use serde::Serialize;

use crate::middleware::auth::ApiKeyInfo;

const IP_RATE_LIMIT_RPM: u32 = 120;

pub struct TokenBucket {
    pub capacity: f64,
    pub tokens: f64,
    pub last_refill: Instant,
    pub refill_rate: f64,
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            last_refill: Instant::now(),
            refill_rate,
        }
    }

    pub fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = Instant::now();
    }

    pub fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn retry_after_secs(&self) -> f64 {
        if self.tokens >= 1.0 {
            0.0
        } else {
            (1.0 - self.tokens) / self.refill_rate
        }
    }
}

#[derive(Clone)]
pub struct RateLimitState(pub Arc<DashMap<String, TokenBucket>>);

impl RateLimitState {
    pub fn new() -> Self {
        Self(Arc::new(DashMap::new()))
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct RateLimitError {
    error: &'static str,
    retry_after_s: f64,
}

fn extract_client_ip(req: &Request) -> String {
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(val) = forwarded.to_str() {
            if let Some(first) = val.split(',').next() {
                let ip = first.trim().to_string();
                if !ip.is_empty() {
                    return ip;
                }
            }
        }
    }

    if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return connect_info.0.ip().to_string();
    }

    "unknown".to_string()
}

pub async fn rate_limit_middleware(
    Extension(rate_limit_state): Extension<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let api_key_info = req.extensions().get::<ApiKeyInfo>().cloned();

    let (rate_limit_key, rpm) = match api_key_info {
        Some(info) => (info.api_key.clone(), info.rate_limit_rpm),
        None => {
            let ip = extract_client_ip(&req);
            (format!("ip:{ip}"), IP_RATE_LIMIT_RPM)
        }
    };

    let refill_rate = rpm as f64 / 60.0;
    let capacity = rpm as f64;

    let (allowed, retry_after) = {
        let mut entry = rate_limit_state
            .0
            .entry(rate_limit_key)
            .or_insert_with(|| TokenBucket::new(capacity, refill_rate));

        let retry_after = entry.retry_after_secs();
        let allowed = entry.try_consume();
        (allowed, retry_after)
    };

    if allowed {
        return next.run(req).await;
    }

    let body = serde_json::to_string(&RateLimitError {
        error: "rate limit exceeded",
        retry_after_s: retry_after,
    })
    .unwrap_or_default();

    let retry_header = format!("{}", retry_after.ceil() as u64);

    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            ("content-type", "application/json"),
            ("Retry-After", &retry_header),
        ],
        body,
    )
        .into_response()
}
