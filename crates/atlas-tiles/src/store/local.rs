use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use pmtiles::{AsyncPmTilesReader, MmapBackend, NoCache};

use super::{tile_type_to_format, TileResponse, TileStore};
use atlas_core::{AtlasError, TileCoord, TileFormat};

type PmtReader = AsyncPmTilesReader<MmapBackend, NoCache>;

struct TilesetEntry {
    reader: PmtReader,
    format: TileFormat,
}

pub struct LocalStore {
    tilesets: HashMap<String, TilesetEntry>,
}

impl LocalStore {
    pub fn empty() -> Self {
        Self {
            tilesets: HashMap::new(),
        }
    }

    pub async fn open(tile_dir: &Path) -> Result<Self, AtlasError> {
        let mut tilesets = HashMap::new();

        let mut entries = tokio::fs::read_dir(tile_dir)
            .await
            .map_err(|e| AtlasError::ConfigError(format!("cannot read tile dir: {e}")))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AtlasError::StoreError(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pmtiles") {
                let tileset = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| AtlasError::ConfigError("invalid filename".into()))?
                    .to_string();

                tracing::info!(tileset = %tileset, path = %path.display(), "loading PMTiles");
                let reader = AsyncPmTilesReader::new_with_path(&path)
                    .await
                    .map_err(|e| {
                        AtlasError::StoreError(format!("failed to open {}: {e}", path.display()))
                    })?;

                let format = tile_type_to_format(reader.get_header().tile_type);
                tilesets.insert(tileset, TilesetEntry { reader, format });
            }
        }

        if tilesets.is_empty() {
            return Err(AtlasError::ConfigError(format!(
                "no .pmtiles files found in {}",
                tile_dir.display()
            )));
        }

        tracing::info!(count = tilesets.len(), "loaded tilesets");
        Ok(Self { tilesets })
    }
}

#[async_trait]
impl TileStore for LocalStore {
    async fn get_tile(
        &self,
        tileset: &str,
        coord: TileCoord,
    ) -> Result<Option<TileResponse>, AtlasError> {
        let entry = self.tilesets.get(tileset).ok_or(AtlasError::TileNotFound)?;

        let pmt_coord: pmtiles::TileCoord = coord.into();

        let tile_data = entry
            .reader
            .get_tile_decompressed(pmt_coord)
            .await
            .map_err(|e| AtlasError::StoreError(e.to_string()))?;

        Ok(tile_data.map(|data| TileResponse {
            data,
            format: entry.format,
        }))
    }

    async fn get_tilejson(
        &self,
        tileset: &str,
        public_url: &str,
    ) -> Result<tilejson::TileJSON, AtlasError> {
        let entry = self.tilesets.get(tileset).ok_or(AtlasError::TileNotFound)?;

        let tile_url = format!(
            "{}/v1/tiles/{}/{{z}}/{{x}}/{{y}}.mvt",
            public_url.trim_end_matches('/'),
            tileset,
        );

        entry
            .reader
            .parse_tilejson(vec![tile_url])
            .await
            .map_err(|e| AtlasError::StoreError(e.to_string()))
    }

    fn tilesets(&self) -> Vec<String> {
        self.tilesets.keys().cloned().collect()
    }
}
