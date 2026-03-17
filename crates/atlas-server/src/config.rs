use atlas_core::AtlasError;

#[derive(Debug, Clone)]
pub struct Config {
    pub tile_source: TileSource,
    pub tile_dir: String,
    pub s3_bucket: Option<String>,
    pub s3_region: String,
    pub s3_tile_keys: Vec<String>,
    pub cache_size_mb: usize,
    pub port: u16,
    pub public_url: String,
    pub geocode_index_dir: String,
    pub landmark_path: String,
    pub places_path: String,
    pub route_dir: String,
    pub osm_dir: String,
    pub search_index_dir: String,
    pub auth_enabled: bool,
    pub dynamodb_table: String,
    pub dynamodb_region: String,
    pub contributions_dir: String,
    pub telemetry_dir: String,
    pub speed_data_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TileSource {
    Local,
    S3,
}

impl Config {
    pub fn from_env() -> Result<Self, AtlasError> {
        let tile_source = match std::env::var("ATLAS_TILE_SOURCE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "local" => TileSource::Local,
            "s3" => TileSource::S3,
            other => {
                return Err(AtlasError::ConfigError(format!(
                    "invalid ATLAS_TILE_SOURCE: {other}, expected 'local' or 's3'"
                )))
            }
        };

        let port: u16 = std::env::var("ATLAS_PORT")
            .unwrap_or_else(|_| "3001".to_string())
            .parse()
            .map_err(|e| AtlasError::ConfigError(format!("invalid ATLAS_PORT: {e}")))?;

        let cache_size_mb: usize = std::env::var("ATLAS_CACHE_SIZE_MB")
            .unwrap_or_else(|_| "256".to_string())
            .parse()
            .map_err(|e| AtlasError::ConfigError(format!("invalid ATLAS_CACHE_SIZE_MB: {e}")))?;

        let public_url = std::env::var("ATLAS_PUBLIC_URL")
            .unwrap_or_else(|_| format!("http://localhost:{port}"));

        let auth_enabled = std::env::var("ATLAS_AUTH_ENABLED")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        Ok(Self {
            tile_source,
            tile_dir: std::env::var("ATLAS_TILE_DIR").unwrap_or_else(|_| "./test-data".to_string()),
            s3_bucket: std::env::var("ATLAS_S3_BUCKET").ok(),
            s3_region: std::env::var("ATLAS_S3_REGION")
                .unwrap_or_else(|_| "af-south-1".to_string()),
            s3_tile_keys: std::env::var("ATLAS_S3_TILE_KEYS")
                .unwrap_or_else(|_| "ghana-basemap.pmtiles".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            cache_size_mb,
            port,
            public_url,
            geocode_index_dir: std::env::var("ATLAS_GEOCODE_INDEX_DIR")
                .unwrap_or_else(|_| "./test-data/geocode-index".to_string()),
            landmark_path: std::env::var("ATLAS_LANDMARK_PATH")
                .unwrap_or_else(|_| "./test-data/landmarks.bin".to_string()),
            places_path: std::env::var("ATLAS_PLACES_PATH")
                .unwrap_or_else(|_| "./test-data/places.bin".to_string()),
            route_dir: std::env::var("ATLAS_ROUTE_DIR")
                .unwrap_or_else(|_| "./test-data".to_string()),
            osm_dir: std::env::var("ATLAS_OSM_DIR").unwrap_or_else(|_| "./data/osm".to_string()),
            search_index_dir: std::env::var("ATLAS_SEARCH_INDEX_DIR")
                .unwrap_or_else(|_| "./test-data/search-index".to_string()),
            auth_enabled,
            dynamodb_table: std::env::var("ATLAS_DYNAMODB_TABLE")
                .unwrap_or_else(|_| "atlas_api_keys".to_string()),
            dynamodb_region: std::env::var("ATLAS_DYNAMODB_REGION")
                .unwrap_or_else(|_| "af-south-1".to_string()),
            contributions_dir: std::env::var("ATLAS_CONTRIBUTIONS_DIR")
                .unwrap_or_else(|_| "./contributions".to_string()),
            telemetry_dir: std::env::var("ATLAS_TELEMETRY_DIR")
                .unwrap_or_else(|_| "./telemetry".to_string()),
            speed_data_path: std::env::var("ATLAS_SPEED_DATA_PATH")
                .unwrap_or_else(|_| "./test-data/speed_data.bin".to_string()),
        })
    }
}
