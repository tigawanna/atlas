use std::fs::File;
use std::path::Path;

use atlas_core::AtlasError;
use flate2::write::GzEncoder;
use flate2::Compression;
use pmtiles::{PmTilesWriter as PmtBuilder, TileCoord, TileType};

struct PendingTile {
    z: u8,
    x: u32,
    y: u32,
    data: Vec<u8>,
}

pub struct PmTilesWriter {
    tiles: Vec<PendingTile>,
    min_zoom: u8,
    max_zoom: u8,
}

impl PmTilesWriter {
    pub fn new() -> Self {
        Self {
            tiles: Vec::new(),
            min_zoom: u8::MAX,
            max_zoom: 0,
        }
    }

    pub fn add_tile(&mut self, z: u8, x: u32, y: u32, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }

        self.min_zoom = self.min_zoom.min(z);
        self.max_zoom = self.max_zoom.max(z);
        self.tiles.push(PendingTile { z, x, y, data });
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn finish(mut self, output: &Path) -> Result<(), AtlasError> {
        if self.tiles.is_empty() {
            return Err(AtlasError::StoreError("no tiles to write".into()));
        }

        self.tiles
            .sort_by_key(|t| pmtiles::TileId::from(TileCoord::new(t.z, t.x, t.y).unwrap()));

        let file = File::create(output)
            .map_err(|e| AtlasError::StoreError(format!("cannot create output file: {e}")))?;

        let mut writer = PmtBuilder::new(TileType::Mvt)
            .min_zoom(self.min_zoom)
            .max_zoom(self.max_zoom)
            .bounds(-25.0, -35.0, 55.0, 38.0)
            .center(15.0, 1.5)
            .center_zoom(4)
            .metadata(r#"{"vector_layers":[{"id":"default","fields":{}}]}"#)
            .create(file)
            .map_err(|e| AtlasError::StoreError(format!("pmtiles writer init failed: {e}")))?;

        for tile in &self.tiles {
            let coord = TileCoord::new(tile.z, tile.x, tile.y)
                .map_err(|e| AtlasError::StoreError(format!("invalid tile coord: {e}")))?;

            let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
            std::io::Write::write_all(&mut gz, &tile.data)
                .map_err(|e| AtlasError::StoreError(format!("gzip compress failed: {e}")))?;
            let compressed = gz
                .finish()
                .map_err(|e| AtlasError::StoreError(format!("gzip finish failed: {e}")))?;

            writer
                .add_raw_tile(coord, &compressed)
                .map_err(|e| AtlasError::StoreError(format!("add tile failed: {e}")))?;
        }

        writer
            .finalize()
            .map_err(|e| AtlasError::StoreError(format!("pmtiles finalize failed: {e}")))?;

        Ok(())
    }
}

impl Default for PmTilesWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn writer_rejects_empty() {
        let tmp = NamedTempFile::with_suffix(".pmtiles").unwrap();
        let writer = PmTilesWriter::new();
        assert!(writer.finish(tmp.path()).is_err());
    }

    #[test]
    fn writer_produces_valid_file() {
        let tmp = NamedTempFile::with_suffix(".pmtiles").unwrap();
        let path = tmp.path().to_path_buf();

        let mut writer = PmTilesWriter::new();
        writer.add_tile(0, 0, 0, vec![0x1a, 0x00]);
        writer.add_tile(1, 0, 0, vec![0x1a, 0x01]);
        writer.add_tile(1, 1, 0, vec![0x1a, 0x02]);

        writer.finish(&path).expect("should write pmtiles");

        let file_size = std::fs::metadata(&path).unwrap().len();
        assert!(file_size > 127, "file should be larger than PMTiles header");
    }

    #[test]
    fn writer_skips_empty_tile_data() {
        let mut writer = PmTilesWriter::new();
        writer.add_tile(0, 0, 0, vec![]);
        assert_eq!(writer.tile_count(), 0);
    }

    #[test]
    fn writer_tracks_zoom_range() {
        let mut writer = PmTilesWriter::new();
        writer.add_tile(3, 0, 0, vec![0x1a]);
        writer.add_tile(7, 0, 0, vec![0x1a]);
        writer.add_tile(5, 0, 0, vec![0x1a]);

        assert_eq!(writer.min_zoom, 3);
        assert_eq!(writer.max_zoom, 7);
    }
}
