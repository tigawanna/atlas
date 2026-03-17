use std::path::PathBuf;

use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Args)]
pub struct BuildArgs {
    #[arg(long, default_value = "./data")]
    data_dir: String,

    #[arg(long, default_value = "./output")]
    output_dir: String,

    #[arg(long)]
    force: bool,

    #[arg(long, default_value = "ghana")]
    region: String,
}

pub fn run(args: BuildArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data_dir = PathBuf::from(&args.data_dir);
    let output_dir = PathBuf::from(&args.output_dir);
    let osm_dir = data_dir.clone();
    let overture_dir = data_dir.join("overture");

    std::fs::create_dir_all(&output_dir)?;

    let spinner_style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

    let spinner = new_spinner(
        &spinner_style,
        "Reading and normalizing OSM + Overture data...",
    );
    let result = atlas_ingest::read_and_normalize(&overture_dir, &osm_dir)?;
    spinner.finish_with_message(format!(
        "Loaded {} places ({} overture, {} osm)",
        result.places.len(),
        result.overture_count,
        result.osm_count,
    ));

    let spinner = new_spinner(&spinner_style, "Building geocode index...");
    let built = atlas_ingest::build_geocode_index(&result.places, &output_dir, args.force)?;
    spinner.finish_with_message(if built {
        "Geocode index built".to_string()
    } else {
        "Geocode index skipped (already exists)".to_string()
    });

    let spinner = new_spinner(&spinner_style, "Building search index...");
    let built = atlas_ingest::build_search_index(&result.places, &output_dir, args.force)?;
    spinner.finish_with_message(if built {
        "Search index built".to_string()
    } else {
        "Search index skipped (already exists)".to_string()
    });

    let spinner = new_spinner(&spinner_style, "Building CH routing graphs...");
    let built = atlas_ingest::build_ch_graphs(&osm_dir, &output_dir, args.force)?;
    spinner.finish_with_message(if built {
        "CH graphs built".to_string()
    } else {
        "CH graphs skipped (already exists)".to_string()
    });

    let spinner = new_spinner(&spinner_style, "Generating PMTiles...");
    let built = atlas_ingest::generate_pmtiles(&osm_dir, &output_dir, &args.region, args.force)?;
    spinner.finish_with_message(if built {
        format!("PMTiles generated: {}-basemap.pmtiles", args.region)
    } else {
        "PMTiles skipped (already exists)".to_string()
    });

    let spinner = new_spinner(&spinner_style, "Saving landmarks and places...");
    atlas_ingest::save_landmarks_and_places(&result.places, &output_dir)?;
    spinner.finish_with_message("Landmarks and places saved");

    println!();
    println!("=== Build Complete ===");
    println!("Places:     {}", result.places.len());
    println!("Output dir: {}", output_dir.display());

    Ok(())
}

fn new_spinner(style: &ProgressStyle, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(style.clone());
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}
