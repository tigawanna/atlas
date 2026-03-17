use std::path::Path;

use atlas_core::geo_utils::haversine_distance;
use atlas_core::{AtlasError, PlacePoint, ReverseOpts, ReverseResult};
use rstar::{RTree, AABB};

pub struct ReverseGeocoder {
    rtree: RTree<PlacePoint>,
}

impl ReverseGeocoder {
    pub fn load(path: &Path) -> Result<Self, AtlasError> {
        let bytes =
            std::fs::read(path).map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        let places: Vec<PlacePoint> = bincode::deserialize(&bytes)
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        let rtree = RTree::bulk_load(places);
        Ok(Self { rtree })
    }

    pub fn save(places: &[PlacePoint], path: &Path) -> Result<(), AtlasError> {
        let bytes =
            bincode::serialize(places).map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?;
        std::fs::write(path, bytes).map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))
    }

    pub fn reverse(&self, lat: f64, lon: f64, opts: &ReverseOpts) -> Vec<ReverseResult> {
        let deg = opts.radius_m / 111_320.0;
        let envelope = AABB::from_corners([lon - deg, lat - deg], [lon + deg, lat + deg]);

        let mut results: Vec<(f64, &PlacePoint)> = self
            .rtree
            .locate_in_envelope_intersecting(&envelope)
            .map(|place| {
                let dist = haversine_distance(lat, lon, place.lat, place.lon);
                (dist, place)
            })
            .filter(|(dist, _)| *dist <= opts.radius_m)
            .collect();

        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        results
            .into_iter()
            .take(opts.limit)
            .map(|(dist, place)| ReverseResult {
                name: place.name.clone(),
                lat: place.lat,
                lon: place.lon,
                distance_m: dist,
                category: place.category.as_str().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::{Category, Lang};

    fn make_place_point(name: &str, lat: f64, lon: f64) -> PlacePoint {
        PlacePoint {
            lat,
            lon,
            name: name.to_string(),
            category: Category::Market,
            address_summary: None,
            names: vec![(Lang::En, name.to_string())],
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.bin");
        let places = vec![
            make_place_point("Makola Market", 5.55, -0.21),
            make_place_point("Kumasi Central", 6.69, -1.62),
        ];
        ReverseGeocoder::save(&places, &path).unwrap();
        let geocoder = ReverseGeocoder::load(&path).unwrap();
        let opts = ReverseOpts {
            limit: 5,
            lang: None,
            radius_m: 10_000.0,
        };
        let results = geocoder.reverse(5.55, -0.21, &opts);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Makola Market");
    }

    #[test]
    fn reverse_finds_nearest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.bin");
        let places = vec![
            make_place_point("Near Place", 5.55, -0.21),
            make_place_point("Far Place", 5.90, -0.50),
        ];
        ReverseGeocoder::save(&places, &path).unwrap();
        let geocoder = ReverseGeocoder::load(&path).unwrap();
        let opts = ReverseOpts::default();
        let results = geocoder.reverse(5.55, -0.21, &opts);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Near Place");
    }

    #[test]
    fn respects_radius_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.bin");
        let places = vec![
            make_place_point("Close", 5.551, -0.211),
            make_place_point("Far Away", 6.0, 0.0),
        ];
        ReverseGeocoder::save(&places, &path).unwrap();
        let geocoder = ReverseGeocoder::load(&path).unwrap();
        let opts = ReverseOpts {
            limit: 10,
            lang: None,
            radius_m: 500.0,
        };
        let results = geocoder.reverse(5.55, -0.21, &opts);
        assert!(results.iter().all(|r| r.distance_m <= 500.0));
        assert!(!results.iter().any(|r| r.name == "Far Away"));
    }

    #[test]
    fn results_sorted_by_distance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("places.bin");
        let places = vec![
            make_place_point("Second", 5.553, -0.213),
            make_place_point("First", 5.551, -0.211),
            make_place_point("Third", 5.560, -0.220),
        ];
        ReverseGeocoder::save(&places, &path).unwrap();
        let geocoder = ReverseGeocoder::load(&path).unwrap();
        let opts = ReverseOpts {
            limit: 10,
            lang: None,
            radius_m: 5000.0,
        };
        let results = geocoder.reverse(5.55, -0.21, &opts);
        for window in results.windows(2) {
            assert!(window[0].distance_m <= window[1].distance_m);
        }
    }

    #[test]
    fn haversine_accra_to_kumasi() {
        let accra_lat = 5.603;
        let accra_lon = -0.187;
        let kumasi_lat = 6.687;
        let kumasi_lon = -1.624;
        let dist = haversine_distance(accra_lat, accra_lon, kumasi_lat, kumasi_lon);
        assert!(
            dist > 180_000.0 && dist < 280_000.0,
            "expected ~200-240km, got {dist}"
        );
    }
}
