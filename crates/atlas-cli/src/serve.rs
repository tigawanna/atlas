use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    #[arg(long, default_value = "./output")]
    data_dir: String,

    #[arg(long, default_value = "3001")]
    port: u16,
}

pub fn run(args: ServeArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let atlas_server =
        std::env::var("ATLAS_SERVER_BIN").unwrap_or_else(|_| "atlas-server".to_string());

    let status = std::process::Command::new(&atlas_server)
        .env("ATLAS_TILE_DIR", &args.data_dir)
        .env(
            "ATLAS_GEOCODE_INDEX_DIR",
            format!("{}/geocode-index", args.data_dir),
        )
        .env(
            "ATLAS_LANDMARK_PATH",
            format!("{}/landmarks.bin", args.data_dir),
        )
        .env("ATLAS_PLACES_PATH", format!("{}/places.bin", args.data_dir))
        .env("ATLAS_ROUTE_DIR", &args.data_dir)
        .env(
            "ATLAS_SEARCH_INDEX_DIR",
            format!("{}/search-index", args.data_dir),
        )
        .env("ATLAS_PORT", args.port.to_string())
        .env("ATLAS_TILE_SOURCE", "local")
        .status()
        .map_err(|e| format!("failed to spawn {atlas_server}: {e}"))?;

    if !status.success() {
        return Err(format!("atlas-server exited with status {status}").into());
    }

    Ok(())
}
