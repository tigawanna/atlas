use std::collections::HashMap;

use async_trait::async_trait;
use aws_sdk_s3::Client;
use pmtiles::{AsyncPmTilesReader, AwsS3Backend, NoCache};

use super::{tile_type_to_format, TileResponse, TileStore};
use atlas_core::{AtlasError, TileCoord, TileFormat};

type S3PmtReader = AsyncPmTilesReader<AwsS3Backend, NoCache>;

struct TilesetEntry {
    reader: S3PmtReader,
    format: TileFormat,
}

pub struct S3Store {
    tilesets: HashMap<String, TilesetEntry>,
}

impl S3Store {
    pub async fn open(client: Client, bucket: &str, keys: &[String]) -> Result<Self, AtlasError> {
        let mut tilesets = HashMap::new();

        for key in keys {
            let tileset = key
                .strip_suffix(".pmtiles")
                .unwrap_or(key.as_str())
                .to_string();

            tracing::info!(tileset = %tileset, key = %key, bucket = %bucket, "loading S3 PMTiles");

            let reader = AsyncPmTilesReader::new_with_client_bucket_and_path(
                client.clone(),
                bucket.to_string(),
                key.clone(),
            )
            .await
            .map_err(|e| {
                AtlasError::StoreError(format!("failed to open s3://{bucket}/{key}: {e}"))
            })?;

            let format = tile_type_to_format(reader.get_header().tile_type);
            tilesets.insert(tileset, TilesetEntry { reader, format });
        }

        if tilesets.is_empty() {
            return Err(AtlasError::ConfigError(
                "no S3 tile keys provided".to_string(),
            ));
        }

        tracing::info!(count = tilesets.len(), "loaded S3 tilesets");
        Ok(Self { tilesets })
    }
}

#[async_trait]
impl TileStore for S3Store {
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
