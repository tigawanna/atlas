mod build;
mod download;
mod serve;
mod upload;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "atlas", about = "Atlas unified CLI pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Download(download::DownloadArgs),
    Build(build::BuildArgs),
    Upload(upload::UploadArgs),
    Serve(serve::ServeArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "atlas=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Download(args) => download::run(args).await?,
        Command::Build(args) => build::run(args)?,
        Command::Upload(args) => upload::run(args).await?,
        Command::Serve(args) => serve::run(args)?,
    }

    Ok(())
}
