use atlas_core::TileFormat;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct TileResponse {
    pub data: Bytes,
    pub format: TileFormat,
}
