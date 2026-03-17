use std::path::Path;
use std::sync::Arc;

use atlas_core::{AtlasError, GeocodeOpts, GeocodeResult, ReverseOpts, ReverseResult};

use crate::index::GeocodeIndex;
use crate::landmark::LandmarkGraph;
use crate::parser::parse;
use crate::reverse::ReverseGeocoder;

pub struct Geocoder {
    index: GeocodeIndex,
    landmarks: LandmarkGraph,
    reverse_geocoder: ReverseGeocoder,
}

impl Geocoder {
    pub fn new(
        index_dir: &Path,
        landmark_path: &Path,
        places_path: &Path,
    ) -> Result<Self, AtlasError> {
        let index = GeocodeIndex::open(index_dir)?;
        let landmarks = LandmarkGraph::load(landmark_path)?;
        let reverse_geocoder = ReverseGeocoder::load(places_path)?;
        Ok(Self {
            index,
            landmarks,
            reverse_geocoder,
        })
    }

    pub async fn geocode(
        self: &Arc<Self>,
        query: &str,
        opts: &GeocodeOpts,
    ) -> Result<Vec<GeocodeResult>, AtlasError> {
        let query = query.to_string();
        let opts = opts.clone();
        let geocoder = Arc::clone(self);

        tokio::task::spawn_blocking(move || {
            let parsed = parse(&query, opts.lang.as_ref());

            let search_text = if !parsed.tokens.is_empty() {
                parsed.tokens.join(" ")
            } else if let Some(ref street) = parsed.street {
                street.clone()
            } else {
                query.clone()
            };

            let mut results = geocoder.index.search(&search_text, &opts)?;

            if let Some(ref lref) = parsed.landmark_ref {
                let low_confidence = results.iter().all(|r| r.confidence < 0.5);
                if low_confidence || results.is_empty() {
                    let bbox: Option<atlas_core::BBox> = None;

                    let found_landmarks =
                        geocoder.landmarks.find_by_name(&lref.name, bbox.as_ref());

                    if let Some(landmark) = found_landmarks.first() {
                        let locality_center: Option<[f64; 2]> = None;
                        let area = geocoder.landmarks.resolve_relation(
                            landmark,
                            &lref.relation,
                            locality_center,
                        );

                        let mut area_opts = opts.clone();
                        area_opts.limit = opts.limit;
                        let area_results = geocoder.index.search(&lref.name, &area_opts)?;
                        let area_results: Vec<GeocodeResult> = area_results
                            .into_iter()
                            .filter(|r| area.contains(r.lon, r.lat))
                            .collect();

                        for r in area_results {
                            if !results.iter().any(|existing| existing.name == r.name) {
                                results.push(r);
                            }
                        }
                    }
                }
            }

            Ok(results)
        })
        .await
        .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?
    }

    pub async fn reverse(
        self: &Arc<Self>,
        lat: f64,
        lon: f64,
        opts: &ReverseOpts,
    ) -> Result<Vec<ReverseResult>, AtlasError> {
        let opts = opts.clone();
        let geocoder = Arc::clone(self);

        tokio::task::spawn_blocking(move || Ok(geocoder.reverse_geocoder.reverse(lat, lon, &opts)))
            .await
            .map_err(|e| AtlasError::GeocodeIndexError(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::GeocodeIndex;
    use crate::landmark::LandmarkGraph;
    use crate::reverse::ReverseGeocoder;
    use atlas_core::{
        Address, Category, LandmarkPoint, Lang, OsmId, Place, PlaceId, PlacePoint, Source,
    };

    fn make_place(name: &str, lat: f64, lon: f64) -> Place {
        Place {
            id: PlaceId::Osm(OsmId::Node(1)),
            names: vec![(Lang::En, name.to_string())],
            category: Category::Market,
            lat,
            lon,
            address: Some(Address {
                street: None,
                city: Some("Accra".to_string()),
                region: None,
                postcode: None,
                country: "Ghana".to_string(),
            }),
            source: Source::Osm,
        }
    }

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

    fn make_landmark_point(name: &str, lat: f64, lon: f64) -> LandmarkPoint {
        LandmarkPoint {
            lat,
            lon,
            names: vec![(Lang::En, name.to_string())],
            category: Category::TelecomTower,
        }
    }

    fn setup_geocoder() -> (Arc<Geocoder>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();

        let places = vec![make_place("Makola Market", 5.55, -0.21)];
        GeocodeIndex::build(&places, dir.path()).unwrap();

        let landmark_path = dir.path().join("landmarks.bin");
        let landmarks = vec![make_landmark_point("MTN Mast", 5.56, -0.20)];
        LandmarkGraph::save(&landmarks, &landmark_path).unwrap();

        let places_path = dir.path().join("places.bin");
        let place_points = vec![make_place_point("Makola Market", 5.55, -0.21)];
        ReverseGeocoder::save(&place_points, &places_path).unwrap();

        let geocoder = Geocoder::new(dir.path(), &landmark_path, &places_path).unwrap();
        (Arc::new(geocoder), dir)
    }

    #[tokio::test]
    async fn geocode_returns_results() {
        let (geocoder, _dir) = setup_geocoder();
        let opts = GeocodeOpts::default();
        let results = geocoder.geocode("Makola Market", &opts).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].name.contains("Makola"));
    }

    #[tokio::test]
    async fn reverse_returns_nearest() {
        let (geocoder, _dir) = setup_geocoder();
        let opts = ReverseOpts {
            limit: 5,
            lang: None,
            radius_m: 10_000.0,
        };
        let results = geocoder.reverse(5.55, -0.21, &opts).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "Makola Market");
    }

    #[tokio::test]
    async fn geocode_empty_query_returns_empty() {
        let (geocoder, _dir) = setup_geocoder();
        let opts = GeocodeOpts::default();
        let results = geocoder.geocode("", &opts).await.unwrap();
        assert!(results.is_empty());
    }
}
