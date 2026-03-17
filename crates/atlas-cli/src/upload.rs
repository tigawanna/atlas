use std::path::PathBuf;

use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Args)]
pub struct UploadArgs {
    #[arg(long, default_value = "./output")]
    output_dir: String,

    #[arg(long)]
    s3_bucket: String,

    #[arg(long, default_value = "af-south-1")]
    s3_region: String,
}

pub async fn run(args: UploadArgs) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output_dir = PathBuf::from(&args.output_dir);

    let aws_config = aws_config::from_env()
        .region(aws_sdk_s3::config::Region::new(args.s3_region.clone()))
        .load()
        .await;
    let client = aws_sdk_s3::Client::new(&aws_config);

    let entries: Vec<PathBuf> = std::fs::read_dir(&output_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();

    if entries.is_empty() {
        tracing::warn!(dir = %output_dir.display(), "no files found to upload");
        return Ok(());
    }

    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {wide_msg}",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    for path in &entries {
        let key = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("invalid filename: {}", path.display()))?
            .to_string();

        pb.set_message(key.clone());

        let body = tokio::fs::read(path).await?;
        let content_type = guess_content_type(&key);

        client
            .put_object()
            .bucket(&args.s3_bucket)
            .key(&key)
            .body(aws_sdk_s3::primitives::ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| format!("upload {key} failed: {e}"))?;

        tracing::info!(key = %key, bucket = %args.s3_bucket, "uploaded");
        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "uploaded {} files to s3://{}",
        entries.len(),
        args.s3_bucket
    ));
    println!(
        "Upload complete: {} files to s3://{}",
        entries.len(),
        args.s3_bucket
    );

    Ok(())
}

fn guess_content_type(filename: &str) -> &'static str {
    if filename.ends_with(".pmtiles") {
        "application/octet-stream"
    } else if filename.ends_with(".json") {
        "application/json"
    } else if filename.ends_with(".bin") {
        "application/octet-stream"
    } else {
        "application/octet-stream"
    }
}
