mod normalize;
mod osm;
mod overture;

use std::path::PathBuf;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::info;

use atlas_route::ch::{preprocess::build_ch, save_ch};
use atlas_route::graph::builder::build_road_graph;
use atlas_route::profiles::{BicycleProfile, CarProfile, FootProfile, MotorcycleProfile};

#[derive(Parser)]
#[command(name = "atlas-ingest", about = "Atlas data ingestion pipeline")]
struct Args {
    #[arg(long, default_value = "./data/overture")]
    overture_dir: String,
    #[arg(long, default_value = "./data/osm")]
    osm_dir: String,
    #[arg(long, default_value = "./test-data")]
    output_dir: String,
    #[arg(long)]
    build_route_graph: bool,
    #[arg(long)]
    build_search_index: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let overture_dir = PathBuf::from(&args.overture_dir);
    let osm_dir = PathBuf::from(&args.osm_dir);
    let output_dir = PathBuf::from(&args.output_dir);

    std::fs::create_dir_all(&output_dir)?;

    if args.build_route_graph {
        return run_route_graph_build(&osm_dir, &output_dir);
    }

    let spinner_style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Reading Overture places...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let overture_places = overture::read_overture_places(&overture_dir)?;
    spinner.finish_with_message(format!("Overture: {} places loaded", overture_places.len()));

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Reading OSM places...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let osm_places = osm::read_osm_places(&osm_dir)?;
    spinner.finish_with_message(format!("OSM: {} places loaded", osm_places.len()));

    if overture_places.is_empty() && osm_places.is_empty() {
        return Err(
            "No places found in Overture or OSM directories. Ensure data files exist.".into(),
        );
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Deduplicating and normalizing...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let places = normalize::deduplicate(overture_places, osm_places);
    spinner.finish_with_message(format!("Normalized: {} unique places", places.len()));

    let landmarks = normalize::extract_landmarks(&places);
    let place_points = normalize::extract_place_points(&places);

    info!(
        total = places.len(),
        landmarks = landmarks.len(),
        place_points = place_points.len(),
        "Extraction complete"
    );

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Building geocode index...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    let geocode_dir = output_dir.join("geocode-index");
    if geocode_dir.exists() {
        tracing::info!(
            "Geocode index already exists, skipping rebuild. Delete {:?} to force rebuild.",
            geocode_dir
        );
    } else {
        atlas_geocode::index::GeocodeIndex::build(&places, &geocode_dir)?;
    }
    spinner.finish_with_message("Geocode index built");

    if args.build_search_index {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(spinner_style.clone());
        spinner.set_message("Building search index...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));
        atlas_search::SearchEngine::build(&places, &output_dir.join("search-index"))?;
        spinner.finish_with_message("Search index built");
    }

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Saving landmark graph...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    atlas_geocode::landmark::LandmarkGraph::save(&landmarks, &output_dir.join("landmarks.bin"))?;
    spinner.finish_with_message("Landmark graph saved");

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(spinner_style.clone());
    spinner.set_message("Saving reverse geocoder data...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    atlas_geocode::reverse::ReverseGeocoder::save(&place_points, &output_dir.join("places.bin"))?;
    spinner.finish_with_message("Reverse geocoder data saved");

    println!();
    println!("=== Atlas Ingest Summary ===");
    println!("Total places:     {}", places.len());
    println!("Landmarks:        {}", landmarks.len());
    println!("Place points:     {}", place_points.len());
    println!("Output directory: {}", output_dir.display());

    Ok(())
}

fn run_route_graph_build(
    osm_dir: &PathBuf,
    output_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Building road graph from OSM data...");
    let (graph, geometry) = build_road_graph(osm_dir)?;
    info!(
        nodes = graph.num_nodes(),
        edges = graph.num_edges(),
        "Road graph built"
    );

    let profiles: Vec<(&str, Box<dyn atlas_route::profiles::RoutingProfile>)> = vec![
        ("car", Box::new(CarProfile)),
        ("motorcycle", Box::new(MotorcycleProfile)),
        ("bicycle", Box::new(BicycleProfile)),
        ("foot", Box::new(FootProfile)),
    ];

    for (name, profile) in &profiles {
        info!(profile = name, "Building CH graph...");
        let ch = build_ch(&graph, &geometry, profile.as_ref());
        let output_path = output_dir.join(format!("ch-{name}.bin"));
        save_ch(&ch, &output_path)?;
        info!(profile = name, path = %output_path.display(), "CH graph saved");
    }

    println!();
    println!("=== Route Graph Build Summary ===");
    println!("Road graph nodes: {}", graph.num_nodes());
    println!("Road graph edges: {}", graph.num_edges());
    println!("Profiles built:   {}", profiles.len());
    println!("Output directory: {}", output_dir.display());

    Ok(())
}
