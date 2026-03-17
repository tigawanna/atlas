use std::path::PathBuf;

use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::CONTENT_LENGTH;

#[derive(Args)]
pub struct DownloadArgs {
    #[arg(long, default_value = "ghana")]
    region: String,

    #[arg(long, default_value = "./data")]
    output_dir: String,
}

pub async fn run(args: DownloadArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output_dir = PathBuf::from(&args.output_dir);
    std::fs::create_dir_all(&output_dir)?;

    let url = format!(
        "https://download.geofabrik.de/africa/{}-latest.osm.pbf",
        args.region
    );

    let dest = output_dir.join(format!("{}-latest.osm.pbf", args.region));

    tracing::info!(url = %url, dest = %dest.display(), "downloading OSM PBF");

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} for {url}", response.status()).into());
    }

    let total_bytes: Option<u64> = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let pb = match total_bytes {
        Some(total) => {
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
            );
            pb
        }
        None => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::with_template("{spinner:.green} {bytes} downloaded").unwrap(),
            );
            pb
        }
    };

    let mut file = tokio::fs::File::create(&dest).await?;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        pb.inc(chunk.len() as u64);
        file.write_all(&chunk).await?;
    }

    pb.finish_with_message(format!("downloaded {}", dest.display()));
    println!("Saved: {}", dest.display());

    Ok(())
}
