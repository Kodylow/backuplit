use std::path::PathBuf;

use backuplit::BackuplitBuilder;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[clap(version = "1.0", author = "Kody Low <kodylow7@gmail.com>")]
struct Cli {
    /// Sets the database path to backup
    #[clap(long, value_name = "DIR_PATH", env = "DIR_PATH", required = true)]
    dir_path: PathBuf,

    /// Sets the bucket ID for the backup
    #[clap(long, value_name = "BUCKET_ID", env = "BUCKET_ID", required = true)]
    bucket_id: String,

    /// Sets the credentials for accessing the storage
    #[clap(long, value_name = "CREDENTIAL", env = "CREDENTIAL", required = true)]
    credential: String,

    /// Sets the name for the backup
    #[clap(long, value_name = "BACKUP_NAME", env = "BACKUP_NAME")]
    backup_name: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Set up the tracing subscriber with environment filter and pretty printing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|e| anyhow::anyhow!("setting default subscriber failed: {}", e))?;

    info!("Application starting up");

    let cli: Cli = Cli::parse();

    info!("Parsed CLI arguments");

    let b = BackuplitBuilder::new()
        .dir_path(cli.dir_path.clone())
        .bucket_id(cli.bucket_id.clone())
        .credential(cli.credential.clone())
        .backup_name(cli.backup_name.clone())
        .build();

    info!("BackuplitBuilder configured with directory path: {:?}, bucket ID: {}, credentials: REDACTED, backup name: {}",
        cli.dir_path, cli.bucket_id, cli.backup_name);

    b.watch_and_backup().await?;

    Ok(())
}
