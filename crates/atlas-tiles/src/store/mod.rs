pub mod cache;
pub mod local;
pub mod response;
pub mod s3;

pub use cache::CachedTileStore;
pub use local::LocalStore;
pub use response::TileResponse;
pub use s3::S3Store;

use atlas_core::{AtlasError, TileCoord, TileFormat};
use tilejson::TileJSON;

pub(crate) fn tile_type_to_format(tile_type: pmtiles::TileType) -> TileFormat {
    match tile_type {
        pmtiles::TileType::Mvt | pmtiles::TileType::Mlt => TileFormat::Mvt,
        pmtiles::TileType::Png => TileFormat::Png,
        pmtiles::TileType::Webp => TileFormat::Webp,
        _ => TileFormat::Mvt,
    }
}

#[async_trait::async_trait]
pub trait TileStore: Send + Sync {
    async fn get_tile(
        &self,
        tileset: &str,
        coord: TileCoord,
    ) -> Result<Option<TileResponse>, AtlasError>;
    async fn get_tilejson(&self, tileset: &str, public_url: &str) -> Result<TileJSON, AtlasError>;
    fn tilesets(&self) -> Vec<String>;
}
