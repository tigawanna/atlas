use atlas_core::{ContributionStore, TelemetryStore};
use atlas_geocode::Geocoder;
use atlas_route::RouteEngine;
use atlas_search::SearchEngine;
use atlas_tiles::TileStore;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub tiles: Arc<dyn TileStore>,
    pub geocoder: Option<Arc<Geocoder>>,
    pub router: Option<Arc<RouteEngine>>,
    pub search: Option<Arc<SearchEngine>>,
    pub contributions: Option<Arc<ContributionStore>>,
    pub telemetry: Option<Arc<TelemetryStore>>,
    pub speed_data_path: Option<PathBuf>,
    pub public_url: String,
    pub ready: Arc<std::sync::atomic::AtomicBool>,
}
