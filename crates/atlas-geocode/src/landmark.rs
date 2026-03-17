use std::path::Path;

use atlas_core::{AtlasError, BBox, LandmarkPoint, SpatialRelation};
use rstar::RTree;

pub struct LandmarkGraph {
    landmarks: RTree<LandmarkPoint>,
}

pub fn meters_to_degrees(meters: f64) -> f64 {
    meters / 111_320.0
}

impl LandmarkGraph {
    pub fn load(path: &Path) -> Result<Self, AtlasError> {
        let bytes =
            std::fs::read(path).map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        let points: Vec<LandmarkPoint> = bincode::deserialize(&bytes)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        let landmarks = RTree::bulk_load(points);
        Ok(Self { landmarks })
    }

    pub fn save(landmarks: &[LandmarkPoint], path: &Path) -> Result<(), AtlasError> {
        let bytes = bincode::serialize(landmarks)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))
    }

    pub fn find_by_name<'a>(
        &'a self,
        name: &str,
        search_bbox: Option<&BBox>,
    ) -> Vec<&'a LandmarkPoint> {
        let name_lower = name.to_lowercase();
        self.landmarks
            .iter()
            .filter(|lp| {
                let name_matches = lp
                    .names
                    .iter()
                    .any(|(_, n)| n.to_lowercase().contains(&name_lower));
                if !name_matches {
                    return false;
                }
                if let Some(bbox) = search_bbox {
                    bbox.contains(lp.lon, lp.lat)
                } else {
                    true
                }
            })
            .collect()
    }

    pub fn resolve_relation(
        &self,
        landmark: &LandmarkPoint,
        relation: &SpatialRelation,
        locality_center: Option<[f64; 2]>,
    ) -> BBox {
        match relation {
            SpatialRelation::Near => {
                let delta = meters_to_degrees(200.0);
                BBox::new(
                    landmark.lon - delta,
                    landmark.lat - delta,
                    landmark.lon + delta,
                    landmark.lat + delta,
                )
            }
            SpatialRelation::Beside => {
                let delta = meters_to_degrees(50.0);
                BBox::new(
                    landmark.lon - delta,
                    landmark.lat - delta,
                    landmark.lon + delta,
                    landmark.lat + delta,
                )
            }
            SpatialRelation::Behind | SpatialRelation::Past | SpatialRelation::Opposite => {
                let center = locality_center.unwrap_or([landmark.lon, landmark.lat]);
                let bearing_lon = landmark.lon - center[0];
                let bearing_lat = landmark.lat - center[1];
                let mag = (bearing_lon * bearing_lon + bearing_lat * bearing_lat).sqrt();
                let (dir_lon, dir_lat) = if mag > 1e-10 {
                    (bearing_lon / mag, bearing_lat / mag)
                } else {
                    (0.0, 1.0)
                };
                let offset = meters_to_degrees(150.0);
                let delta = meters_to_degrees(100.0);
                let center_lon = landmark.lon + dir_lon * offset;
                let center_lat = landmark.lat + dir_lat * offset;
                BBox::new(
                    center_lon - delta,
                    center_lat - delta,
                    center_lon + delta,
                    center_lat + delta,
                )
            }
            SpatialRelation::Between(other_name) => {
                let others = self.find_by_name(other_name, None);
                if let Some(other) = others.first() {
                    let mid_lon = (landmark.lon + other.lon) / 2.0;
                    let mid_lat = (landmark.lat + other.lat) / 2.0;
                    let delta = meters_to_degrees(100.0);
                    BBox::new(
                        mid_lon - delta,
                        mid_lat - delta,
                        mid_lon + delta,
                        mid_lat + delta,
                    )
                } else {
                    let delta = meters_to_degrees(200.0);
                    BBox::new(
                        landmark.lon - delta,
                        landmark.lat - delta,
                        landmark.lon + delta,
                        landmark.lat + delta,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::{Category, Lang};

    fn make_landmark(name: &str, lat: f64, lon: f64) -> LandmarkPoint {
        LandmarkPoint {
            lat,
            lon,
            names: vec![(Lang::En, name.to_string())],
            category: Category::TelecomTower,
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![
            make_landmark("MTN Mast", 5.6, -0.2),
            make_landmark("Vodafone Tower", 5.7, -0.1),
        ];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let found = graph.find_by_name("MTN Mast", None);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn find_by_name_exact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![make_landmark("Accra Mall", 5.6, -0.2)];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let found = graph.find_by_name("Accra Mall", None);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn find_by_name_partial() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![
            make_landmark("Accra Mall", 5.6, -0.2),
            make_landmark("Accra Central Market", 5.55, -0.22),
        ];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let found = graph.find_by_name("Accra", None);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn find_by_name_with_bbox_filters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![
            make_landmark("Tower A", 5.6, -0.2),
            make_landmark("Tower B", 10.0, 30.0),
        ];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let bbox = BBox::new(-1.0, 5.0, 0.0, 6.0);
        let found = graph.find_by_name("Tower", Some(&bbox));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].names[0].1, "Tower A");
    }

    #[test]
    fn resolve_relation_near_bbox_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![make_landmark("Mast", 5.6, -0.2)];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let lm = &graph.find_by_name("Mast", None)[0];
        let bbox = graph.resolve_relation(lm, &SpatialRelation::Near, None);
        let expected_delta = meters_to_degrees(200.0);
        assert!((bbox.max_lon - bbox.min_lon - expected_delta * 2.0).abs() < 1e-10);
        assert!((bbox.max_lat - bbox.min_lat - expected_delta * 2.0).abs() < 1e-10);
    }

    #[test]
    fn resolve_relation_between_returns_midpoint_area() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("landmarks.bin");
        let landmarks = vec![
            make_landmark("Tower A", 5.6, -0.2),
            make_landmark("Tower B", 5.8, -0.4),
        ];
        LandmarkGraph::save(&landmarks, &path).unwrap();
        let graph = LandmarkGraph::load(&path).unwrap();
        let lm_a = graph.find_by_name("Tower A", None)[0];
        let bbox =
            graph.resolve_relation(lm_a, &SpatialRelation::Between("Tower B".to_string()), None);
        let expected_mid_lon = (-0.2 + -0.4) / 2.0;
        let expected_mid_lat = (5.6 + 5.8) / 2.0;
        let center_lon = (bbox.min_lon + bbox.max_lon) / 2.0;
        let center_lat = (bbox.min_lat + bbox.max_lat) / 2.0;
        assert!((center_lon - expected_mid_lon).abs() < 1e-10);
        assert!((center_lat - expected_mid_lat).abs() < 1e-10);
    }
}
