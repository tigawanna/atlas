use atlas_core::geo_utils::haversine_distance;
use atlas_core::{LandmarkPoint, Lang, Place, PlacePoint};
use unicode_normalization::UnicodeNormalization;

const DEDUP_DISTANCE_M: f64 = 50.0;
const DEDUP_NAME_SIMILARITY: f64 = 0.8;

pub fn name_similarity(a: &[(Lang, String)], b: &[(Lang, String)]) -> f64 {
    let mut max_score = 0.0_f64;
    let mut found_shared_lang = false;

    for (lang_a, name_a) in a {
        for (lang_b, name_b) in b {
            if lang_a == lang_b {
                found_shared_lang = true;
                let score = strsim::normalized_levenshtein(
                    &normalize_name(name_a),
                    &normalize_name(name_b),
                );
                max_score = max_score.max(score);
            }
        }
    }

    if !found_shared_lang {
        if let (Some((_, first_a)), Some((_, first_b))) = (a.first(), b.first()) {
            let score =
                strsim::normalized_levenshtein(&normalize_name(first_a), &normalize_name(first_b));
            max_score = max_score.max(score);
        }
    }

    max_score
}

fn normalize_name(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

pub fn deduplicate(overture: Vec<Place>, osm: Vec<Place>) -> Vec<Place> {
    let mut result = overture;

    for osm_place in osm {
        let match_idx = result.iter().position(|overture_place| {
            let dist = haversine_distance(
                overture_place.lat,
                overture_place.lon,
                osm_place.lat,
                osm_place.lon,
            );
            if dist > DEDUP_DISTANCE_M {
                return false;
            }
            name_similarity(&overture_place.names, &osm_place.names) > DEDUP_NAME_SIMILARITY
        });

        match match_idx {
            Some(idx) => {
                merge_osm_into_overture(&mut result[idx], &osm_place);
            }
            None => {
                result.push(osm_place);
            }
        }
    }

    result
}

fn merge_osm_into_overture(overture: &mut Place, osm: &Place) {
    if overture.address.is_none() {
        overture.address = osm.address.clone();
    } else if let (Some(ref mut addr), Some(ref osm_addr)) = (&mut overture.address, &osm.address) {
        if addr.street.is_none() {
            addr.street = osm_addr.street.clone();
        }
        if addr.city.is_none() {
            addr.city = osm_addr.city.clone();
        }
        if addr.region.is_none() {
            addr.region = osm_addr.region.clone();
        }
        if addr.postcode.is_none() {
            addr.postcode = osm_addr.postcode.clone();
        }
    }

    for (lang, name) in &osm.names {
        if !overture.names.iter().any(|(l, _)| l == lang) {
            overture.names.push((lang.clone(), name.clone()));
        }
    }
}

pub fn extract_landmarks(places: &[Place]) -> Vec<LandmarkPoint> {
    places
        .iter()
        .filter(|p| p.category.is_landmark())
        .map(|p| LandmarkPoint {
            lat: p.lat,
            lon: p.lon,
            names: p.names.clone(),
            category: p.category,
        })
        .collect()
}

pub fn extract_place_points(places: &[Place]) -> Vec<PlacePoint> {
    places
        .iter()
        .map(|p| {
            let name = p.primary_name(None).to_string();
            let address_summary = p.address.as_ref().map(|a| a.full_string());
            PlacePoint {
                lat: p.lat,
                lon: p.lon,
                name,
                category: p.category,
                address_summary,
                names: p.names.clone(),
            }
        })
        .collect()
}
