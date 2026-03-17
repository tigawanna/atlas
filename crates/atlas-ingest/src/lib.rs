pub mod normalize;
pub mod osm;
pub mod overture;

use std::path::Path;

use atlas_core::Place;

pub struct IngestResult {
    pub places: Vec<Place>,
    pub overture_count: usize,
    pub osm_count: usize,
}

pub fn read_and_normalize(
    overture_dir: &Path,
    osm_dir: &Path,
) -> Result<IngestResult, Box<dyn std::error::Error + Send + Sync>> {
    let overture_places = overture::read_overture_places(overture_dir)
        .map_err(|e| format!("overture read failed: {e}"))?;
    let osm_places = osm::read_osm_places(osm_dir).map_err(|e| format!("osm read failed: {e}"))?;

    let overture_count = overture_places.len();
    let osm_count = osm_places.len();

    let places = normalize::deduplicate(overture_places, osm_places);

    Ok(IngestResult {
        places,
        overture_count,
        osm_count,
    })
}

pub fn build_geocode_index(
    places: &[Place],
    output_dir: &Path,
    force: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let geocode_dir = output_dir.join("geocode-index");
    if geocode_dir.exists() && !force {
        return Ok(false);
    }
    atlas_geocode::index::GeocodeIndex::build(places, &geocode_dir)
        .map_err(|e| format!("geocode index build failed: {e}"))?;
    Ok(true)
}

pub fn build_search_index(
    places: &[Place],
    output_dir: &Path,
    force: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let search_dir = output_dir.join("search-index");
    if search_dir.exists() && !force {
        return Ok(false);
    }
    atlas_search::SearchEngine::build(places, &search_dir)
        .map_err(|e| format!("search index build failed: {e}"))?;
    Ok(true)
}

pub fn build_ch_graphs(
    osm_dir: &Path,
    output_dir: &Path,
    force: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    use atlas_route::ch::{preprocess::build_ch, save_ch};
    use atlas_route::graph::builder::build_road_graph;
    use atlas_route::profiles::{BicycleProfile, CarProfile, FootProfile, MotorcycleProfile};

    let profiles: &[(&str, &dyn atlas_route::profiles::RoutingProfile)] = &[
        ("car", &CarProfile),
        ("motorcycle", &MotorcycleProfile),
        ("bicycle", &BicycleProfile),
        ("foot", &FootProfile),
    ];

    let all_exist = profiles
        .iter()
        .all(|(name, _)| output_dir.join(format!("ch-{name}.bin")).exists());

    if all_exist && !force {
        return Ok(false);
    }

    let (graph, geometry) =
        build_road_graph(osm_dir).map_err(|e| format!("road graph build failed: {e}"))?;

    for (name, profile) in profiles {
        let output_path = output_dir.join(format!("ch-{name}.bin"));
        if output_path.exists() && !force {
            continue;
        }
        let ch = build_ch(&graph, &geometry, *profile);
        save_ch(&ch, &output_path).map_err(|e| format!("save CH graph {name} failed: {e}"))?;
    }

    Ok(true)
}

pub fn save_landmarks_and_places(
    places: &[Place],
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let landmarks = normalize::extract_landmarks(places);
    let place_points = normalize::extract_place_points(places);

    atlas_geocode::landmark::LandmarkGraph::save(&landmarks, &output_dir.join("landmarks.bin"))
        .map_err(|e| format!("save landmarks failed: {e}"))?;

    atlas_geocode::reverse::ReverseGeocoder::save(&place_points, &output_dir.join("places.bin"))
        .map_err(|e| format!("save places failed: {e}"))?;

    Ok(())
}

pub fn generate_pmtiles(
    osm_dir: &Path,
    output_dir: &Path,
    region: &str,
    force: bool,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let output_path = output_dir.join(format!("{region}-basemap.pmtiles"));
    if output_path.exists() && !force {
        return Ok(false);
    }

    atlas_tiles::generator::TileGenerator::default()
        .generate(osm_dir, &output_path)
        .map_err(|e| format!("PMTiles generation failed: {e}"))?;

    Ok(true)
}
