pub mod encoder;
pub mod simplify;
pub mod writer;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use atlas_core::bbox::AFRICA;
use atlas_core::AtlasError;
use osmpbf::{Element, ElementReader};
use tracing::{info, warn};

use encoder::{GeomType, PropertyValue, TileFeature};
use simplify::{simplify_line, tile_for_point};
use writer::PmTilesWriter;

const MAX_ZOOM: u8 = 14;

pub struct TileGenerator {
    min_zoom: u8,
    max_zoom: u8,
}

impl TileGenerator {
    pub fn new(min_zoom: u8, max_zoom: u8) -> Self {
        Self {
            min_zoom: min_zoom.min(MAX_ZOOM),
            max_zoom: max_zoom.min(MAX_ZOOM),
        }
    }

    pub fn generate(&self, osm_dir: &Path, output: &Path) -> Result<(), AtlasError> {
        let features = self.read_osm_features(osm_dir)?;
        if features.is_empty() {
            return Err(AtlasError::StoreError(
                "no features extracted from OSM data".into(),
            ));
        }

        info!(count = features.len(), "extracted OSM features");

        let mut pmwriter = PmTilesWriter::new();

        for zoom in self.min_zoom..=self.max_zoom {
            let tiles = self.group_features_by_tile(&features, zoom);
            info!(
                zoom = zoom,
                tile_count = tiles.len(),
                "encoding tiles for zoom"
            );

            for ((x, y), tile_features) in &tiles {
                let simplified_features: Vec<TileFeature> = tile_features
                    .iter()
                    .map(|f| self.simplify_feature(f, zoom))
                    .collect();

                let mvt_bytes = encoder::encode_tile(&simplified_features, "default", zoom, *x, *y);
                if !mvt_bytes.is_empty() {
                    pmwriter.add_tile(zoom, *x, *y, mvt_bytes);
                }
            }
        }

        info!(total_tiles = pmwriter.tile_count(), "writing PMTiles");
        pmwriter.finish(output)?;

        Ok(())
    }

    fn read_osm_features(&self, dir: &Path) -> Result<Vec<TileFeature>, AtlasError> {
        if !dir.exists() {
            warn!(path = %dir.display(), "OSM directory does not exist");
            return Ok(vec![]);
        }

        let pbf_files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| AtlasError::StoreError(format!("cannot read OSM dir: {e}")))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|p| p.to_string_lossy().ends_with(".osm.pbf"))
            .collect();

        if pbf_files.is_empty() {
            warn!(path = %dir.display(), "no .osm.pbf files found");
            return Ok(vec![]);
        }

        let mut all_features = Vec::new();
        let mut next_id = 1u64;

        for path in pbf_files {
            info!(path = %path.display(), "reading OSM PBF");
            let reader = ElementReader::from_path(&path)
                .map_err(|e| AtlasError::StoreError(format!("cannot open PBF: {e}")))?;

            let features_mutex = Mutex::new(Vec::new());

            reader
                .for_each(|element| {
                    if let Some(feat) = extract_feature(&element) {
                        if let Ok(mut guard) = features_mutex.lock() {
                            guard.push(feat);
                        }
                    }
                })
                .map_err(|e| AtlasError::StoreError(format!("PBF read error: {e}")))?;

            let mut collected = features_mutex
                .into_inner()
                .map_err(|e| AtlasError::StoreError(format!("mutex poisoned: {e}")))?;

            for feat in &mut collected {
                feat.id = next_id;
                next_id += 1;
            }

            all_features.extend(collected);
        }

        Ok(all_features)
    }

    fn group_features_by_tile<'a>(
        &self,
        features: &'a [TileFeature],
        zoom: u8,
    ) -> HashMap<(u32, u32), Vec<&'a TileFeature>> {
        let mut tiles: HashMap<(u32, u32), Vec<&'a TileFeature>> = HashMap::new();

        for feat in features {
            if let Some(&(lat, lon)) = feat.geometry.first() {
                let (tx, ty) = tile_for_point(lat, lon, zoom);
                tiles.entry((tx, ty)).or_default().push(feat);
            }
        }

        tiles
    }

    fn simplify_feature(&self, feat: &TileFeature, zoom: u8) -> TileFeature {
        let geometry = match feat.geom_type {
            GeomType::Point => feat.geometry.clone(),
            GeomType::LineString | GeomType::Polygon => {
                let simplified = simplify_line(&feat.geometry, zoom);
                if simplified.len() < 2 {
                    feat.geometry.clone()
                } else {
                    simplified
                }
            }
        };

        TileFeature {
            id: feat.id,
            geom_type: feat.geom_type.clone(),
            geometry,
            properties: feat.properties.clone(),
        }
    }
}

impl Default for TileGenerator {
    fn default() -> Self {
        Self::new(0, MAX_ZOOM)
    }
}

fn extract_node_feature(lat: f64, lon: f64, tags: Vec<(&str, &str)>) -> Option<TileFeature> {
    if !AFRICA.contains(lon, lat) {
        return None;
    }

    if tags.is_empty() {
        return None;
    }

    let geom_type = classify_point(&tags)?;
    let properties = tags_to_properties(&tags);

    Some(TileFeature {
        id: 0,
        geom_type,
        geometry: vec![(lat, lon)],
        properties,
    })
}

fn extract_feature(element: &Element<'_>) -> Option<TileFeature> {
    match element {
        Element::Node(node) => extract_node_feature(node.lat(), node.lon(), node.tags().collect()),
        Element::DenseNode(node) => {
            extract_node_feature(node.lat(), node.lon(), node.tags().collect())
        }
        Element::Way(way) => {
            let tags: Vec<(&str, &str)> = way.tags().collect();
            if tags.is_empty() {
                return None;
            }

            let refs: Vec<_> = way.node_locations().collect();
            if refs.len() < 2 {
                return None;
            }

            let first_in_africa = refs.iter().any(|r| AFRICA.contains(r.lon(), r.lat()));
            if !first_in_africa {
                return None;
            }

            let coords: Vec<(f64, f64)> = refs.iter().map(|r| (r.lat(), r.lon())).collect();

            let is_closed = refs.len() >= 4
                && refs.first().map(|r| (r.lat(), r.lon()))
                    == refs.last().map(|r| (r.lat(), r.lon()));

            let geom_type = classify_way(&tags, is_closed)?;
            let properties = tags_to_properties(&tags);

            Some(TileFeature {
                id: 0,
                geom_type,
                geometry: coords,
                properties,
            })
        }
        _ => None,
    }
}

fn classify_point(tags: &[(&str, &str)]) -> Option<GeomType> {
    let dominated_keys = [
        "place", "amenity", "shop", "tourism", "natural", "historic", "leisure",
    ];

    for &(key, _) in tags {
        if dominated_keys.contains(&key) {
            return Some(GeomType::Point);
        }
    }

    None
}

fn classify_way(tags: &[(&str, &str)], is_closed: bool) -> Option<GeomType> {
    for &(key, _) in tags {
        match key {
            "highway" | "railway" | "waterway" => return Some(GeomType::LineString),
            "building" | "landuse" | "natural" | "leisure" | "amenity" if is_closed => {
                return Some(GeomType::Polygon);
            }
            _ => {}
        }
    }

    if is_closed {
        for &(key, _) in tags {
            if key == "area" {
                return Some(GeomType::Polygon);
            }
        }
    }

    for &(key, _) in tags {
        if key == "highway" || key == "railway" || key == "waterway" {
            return Some(GeomType::LineString);
        }
    }

    None
}

fn tags_to_properties(tags: &[(&str, &str)]) -> Vec<(String, PropertyValue)> {
    let relevant_keys = [
        "name", "highway", "building", "landuse", "natural", "amenity", "shop", "tourism", "place",
        "waterway", "leisure", "railway",
    ];

    tags.iter()
        .filter(|(key, _)| relevant_keys.contains(key))
        .map(|(key, val)| (key.to_string(), PropertyValue::String(val.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::encoder::{GeomType, PropertyValue};
    use tempfile::TempDir;

    fn make_synthetic_features() -> Vec<TileFeature> {
        vec![
            TileFeature {
                id: 1,
                geom_type: GeomType::Point,
                geometry: vec![(5.603, -0.187)],
                properties: vec![(
                    "name".to_string(),
                    PropertyValue::String("Accra".to_string()),
                )],
            },
            TileFeature {
                id: 2,
                geom_type: GeomType::LineString,
                geometry: vec![(5.600, -0.190), (5.605, -0.185), (5.610, -0.180)],
                properties: vec![(
                    "highway".to_string(),
                    PropertyValue::String("primary".to_string()),
                )],
            },
            TileFeature {
                id: 3,
                geom_type: GeomType::Polygon,
                geometry: vec![
                    (5.601, -0.188),
                    (5.602, -0.188),
                    (5.602, -0.187),
                    (5.601, -0.187),
                    (5.601, -0.188),
                ],
                properties: vec![(
                    "building".to_string(),
                    PropertyValue::String("yes".to_string()),
                )],
            },
        ]
    }

    #[test]
    fn tile_generator_with_synthetic_data() {
        let tmp = TempDir::new().unwrap();
        let output = tmp.path().join("test.pmtiles");

        let generator = TileGenerator::new(10, 10);
        let features = make_synthetic_features();

        let mut pmwriter = PmTilesWriter::new();
        for zoom in 10..=10u8 {
            let tiles = generator.group_features_by_tile(&features, zoom);
            for ((x, y), tile_features) in &tiles {
                let simplified: Vec<TileFeature> = tile_features
                    .iter()
                    .map(|f| generator.simplify_feature(f, zoom))
                    .collect();
                let mvt = encoder::encode_tile(&simplified, "default", zoom, *x, *y);
                if !mvt.is_empty() {
                    pmwriter.add_tile(zoom, *x, *y, mvt);
                }
            }
        }

        assert!(pmwriter.tile_count() > 0, "should have generated tiles");
        pmwriter.finish(&output).expect("should write pmtiles");

        let file_size = std::fs::metadata(&output).unwrap().len();
        assert!(file_size > 127);
    }

    #[test]
    fn group_features_assigns_correct_tiles() {
        let features = make_synthetic_features();
        let gen = TileGenerator::new(10, 10);
        let grouped = gen.group_features_by_tile(&features, 10);

        let accra_tile = (511u32, 496u32);
        assert!(
            grouped.contains_key(&accra_tile),
            "Accra features should be in tile (511, 496) at z10"
        );
    }

    #[test]
    fn simplify_preserves_points() {
        let gen = TileGenerator::default();
        let point_feat = TileFeature {
            id: 1,
            geom_type: GeomType::Point,
            geometry: vec![(5.0, -0.1)],
            properties: vec![],
        };

        let simplified = gen.simplify_feature(&point_feat, 5);
        assert_eq!(simplified.geometry.len(), 1);
    }

    #[test]
    fn extract_feature_rejects_empty_tags() {
        let tags_empty: Vec<(&str, &str)> = vec![];
        assert!(classify_point(&tags_empty).is_none());
    }

    #[test]
    fn classify_way_highway_is_linestring() {
        let tags = vec![("highway", "residential")];
        assert_eq!(classify_way(&tags, false), Some(GeomType::LineString));
    }

    #[test]
    fn classify_way_building_closed_is_polygon() {
        let tags = vec![("building", "yes")];
        assert_eq!(classify_way(&tags, true), Some(GeomType::Polygon));
    }

    #[test]
    fn classify_way_building_open_is_none() {
        let tags = vec![("building", "yes")];
        assert_eq!(classify_way(&tags, false), None);
    }

    #[test]
    fn tags_to_properties_filters_relevant() {
        let tags = vec![
            ("name", "Test"),
            ("source", "survey"),
            ("highway", "primary"),
        ];
        let props = tags_to_properties(&tags);
        assert_eq!(props.len(), 2);
        assert!(props.iter().any(|(k, _)| k == "name"));
        assert!(props.iter().any(|(k, _)| k == "highway"));
        assert!(!props.iter().any(|(k, _)| k == "source"));
    }
}
