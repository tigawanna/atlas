use thiserror::Error;

#[derive(Error, Debug)]
pub enum AtlasError {
    #[error("tile not found")]
    TileNotFound,

    #[error("invalid coordinate: {0}")]
    InvalidCoord(String),

    #[error("store error: {0}")]
    StoreError(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("geocode index error: {0}")]
    GeocodeIndexError(String),

    #[error("query parse error: {0}")]
    QueryParseError(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("invalid format: {0}")]
    InvalidFormat(String),
}
