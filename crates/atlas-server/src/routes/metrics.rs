use axum::extract::Extension;
use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusHandle;

pub async fn metrics_handler(Extension(handle): Extension<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}
