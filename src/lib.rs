use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use flate2::write::GzEncoder;
use flate2::Compression;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use inotify::{EventMask, Inotify, WatchMask};
use tar::Builder;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use tracing::info;

#[derive(Clone)]
pub enum BackupTrigger {
    Interval(Duration),
    EventMasks(Vec<EventMask>),
}

impl Default for BackupTrigger {
    fn default() -> Self {
        let default_masks = vec![
            EventMask::CLOSE_WRITE,
            EventMask::CREATE,
            EventMask::DELETE,
            EventMask::MODIFY,
            EventMask::CLOSE_NOWRITE,
        ];
        Self::EventMasks(default_masks)
    }
}

#[derive(Clone)]
pub struct Backuplit {
    client: Arc<Mutex<Client>>,
    bucket_name: String,
    dir_path: PathBuf,
    backup_name: String,
    backup_trigger: BackupTrigger,
}

impl Backuplit {
    pub async fn backup_directory_contents(&self) -> Result<(), anyhow::Error> {
        info!("Starting backup of directory contents");
        let dir_path = &self.dir_path;
        let bucket_name = &self.bucket_name;
        let backup_name = &self.backup_name;

        let tarball_data = Vec::new();
        let gz_encoder = GzEncoder::new(tarball_data, Compression::default());
        let mut ar = Builder::new(gz_encoder);
        ar.append_dir_all(backup_name, dir_path)?;

        let gz_encoder = ar.into_inner()?;
        let compressed_tarball_bytes = gz_encoder.finish()?;
        let backup_name_str = backup_name.clone();
        let upload_type = UploadType::Simple(Media::new(backup_name_str));

        self.client
            .lock()
            .await
            .upload_object(
                &UploadObjectRequest {
                    bucket: bucket_name.clone(),
                    ..Default::default()
                },
                compressed_tarball_bytes,
                &upload_type,
            )
            .await?;

        info!("Directory backup completed successfully");

        Ok(())
    }

    async fn handle_interval_trigger(&self, duration: &Duration) -> Result<(), anyhow::Error> {
        loop {
            sleep(*duration).await;
            info!("Interval backup triggered");
            self.backup_directory_contents().await?;
        }
    }

    async fn process_events_and_backup(
        &self,
        inotify: &mut Inotify,
        buffer: &mut [u8; 1024],
        masks: &[EventMask],
        last_backup: &mut Instant,
        debounce_duration: Duration,
    ) -> Result<(), anyhow::Error> {
        let events = inotify
            .read_events_blocking(buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read inotify events: {}", e))?;
        if events
            .into_iter()
            .any(|event| masks.iter().any(|mask| event.mask.contains(*mask)))
        {
            if last_backup.elapsed() >= debounce_duration {
                info!("Debounced backup triggered");
                self.backup_directory_contents().await?;
                *last_backup = Instant::now();
            } else {
                info!("Rate limit enforced, backup skipped");
            }
        }

        Ok(())
    }

    async fn handle_event_mask_trigger(&self, masks: &[EventMask]) -> Result<(), anyhow::Error> {
        let dir_path = self.dir_path.clone();
        let mut inotify =
            Inotify::init().map_err(|e| anyhow::anyhow!("Failed to initialize inotify: {}", e))?;
        let watch_mask = masks.iter().fold(WatchMask::empty(), |acc, mask| {
            acc | WatchMask::from_bits_truncate(mask.bits())
        });
        inotify
            .watches()
            .add(&dir_path, watch_mask)
            .map_err(|e| anyhow::anyhow!("Failed to add watch: {}", e))?;

        let mut buffer = [0; 1024];
        let mut last_backup = Instant::now();
        let debounce_duration = Duration::from_secs(1);
        loop {
            self.process_events_and_backup(
                &mut inotify,
                &mut buffer,
                masks,
                &mut last_backup,
                debounce_duration,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to process events for backup: {}", e))?;
        }
    }

    pub async fn watch(&self) -> Result<(), anyhow::Error> {
        match &self.backup_trigger {
            BackupTrigger::Interval(duration) => {
                info!(?duration, "Starting interval-based backup trigger");
                self.handle_interval_trigger(duration).await
            }
            BackupTrigger::EventMasks(masks) => {
                info!(?masks, "Starting event masks-based backup trigger");
                self.handle_event_mask_trigger(masks).await
            }
        }
    }
}

pub struct BackuplitBuilder {
    pub dir_path: PathBuf,
    pub bucket_name: String,
    pub backup_name: String,
    pub backup_trigger: BackupTrigger,
}

impl BackuplitBuilder {
    pub fn new() -> Self {
        Self {
            dir_path: PathBuf::new(),
            backup_name: String::new(),
            bucket_name: String::new(),
            backup_trigger: BackupTrigger::default(),
        }
    }

    pub fn dir_path(mut self, dir_path: PathBuf) -> Self {
        self.dir_path = dir_path;
        self
    }

    pub fn backup_name(mut self, backup_name: String) -> Self {
        self.backup_name = backup_name;
        self
    }

    pub fn bucket_name(mut self, bucket_name: String) -> Self {
        self.bucket_name = bucket_name;
        self
    }

    pub fn backup_trigger(mut self, trigger: BackupTrigger) -> Self {
        self.backup_trigger = trigger;
        self
    }

    pub async fn build(self) -> Result<Backuplit, anyhow::Error> {
        let config = ClientConfig::default().with_auth().await?;
        let client = Arc::new(Mutex::new(Client::new(config)));
        Ok(Backuplit {
            client,
            dir_path: self.dir_path,
            bucket_name: self.bucket_name,
            backup_name: self.backup_name,
            backup_trigger: self.backup_trigger,
        })
    }
}

impl Default for BackuplitBuilder {
    fn default() -> Self {
        Self::new()
    }
}
