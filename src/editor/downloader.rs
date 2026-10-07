use std::path::PathBuf;
use std::sync::mpsc::Sender;

use bevy::tasks::{IoTaskPool, TaskPool};
use hf_hub::split_id;
use hf_hub::progress::{DownloadEvent, ProgressEvent, ProgressHandler};

use crate::editor::model::Config;

#[derive(Clone, Debug)]
pub enum DownloadProgress {
    CheckingCache,
    CachedFound {
        model: PathBuf,
        mmproj: PathBuf,
        mtp: Option<PathBuf>,
    },
    Downloading {
        file: String,
        current_bytes: u64,
        total_bytes: u64,
        pct: f32,
    },
    Complete {
        model: PathBuf,
        mmproj: PathBuf,
        mtp: Option<PathBuf>,
    },
    Error(String),
}

/// Check if a specific file exists in the local Hugging Face Hub cache for a repo.
pub fn find_file_in_cache(repo_id: &str, substring: &str) -> Option<PathBuf> {
    let home = std::env::var("HF_HOME")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache").join("huggingface"))
        })
        .ok()?;
    let hub_dir = std::env::var("HF_HUB_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join("hub"));

    let repo_dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let repo_dir = hub_dir.join(repo_dir_name);
    let snapshots_dir = repo_dir.join("snapshots");
    if !snapshots_dir.is_dir() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
        for entry in entries.flatten() {
            let snap_path = entry.path();
            if snap_path.is_dir() {
                if let Ok(files) = std::fs::read_dir(&snap_path) {
                    for f in files.flatten() {
                        let path = f.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.ends_with(".gguf") && name.contains(substring) {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Check if models exist at configured paths or in the local Hugging Face Hub cache.
pub fn find_in_local_cache(repo_id: &str) -> Option<(PathBuf, PathBuf, Option<PathBuf>)> {
    let home = std::env::var("HF_HOME")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache").join("huggingface"))
        })
        .ok()?;
    let hub_dir = std::env::var("HF_HUB_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join("hub"));

    // Format: models--owner--name
    let repo_dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let repo_dir = hub_dir.join(repo_dir_name);
    let snapshots_dir = repo_dir.join("snapshots");
    if !snapshots_dir.is_dir() {
        return None;
    }

    // Inspect recent snapshots
    let mut model_file = None;
    let mut mmproj_file = None;
    let mut mtp_file = None;

    if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
        for entry in entries.flatten() {
            let snap_path = entry.path();
            if snap_path.is_dir() {
                if let Ok(files) = std::fs::read_dir(&snap_path) {
                    for f in files.flatten() {
                        let path = f.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.ends_with(".gguf") {
                            if name.contains("mmproj") {
                                mmproj_file = Some(path.clone());
                            } else if name.contains("mtp") {
                                mtp_file = Some(path.clone());
                            } else {
                                model_file = Some(path.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    match (model_file, mmproj_file) {
        (Some(m), Some(p)) => Some((m, p, mtp_file)),
        _ => None,
    }
}

/// Inspects whether models are already resolved on disk or in the HF cache.
pub fn check_models_available(config: &Config) -> Option<(PathBuf, PathBuf, Option<PathBuf>)> {
    let mtp_found = if config.enable_mtp {
        if config.mtp_path.is_file() {
            Some(config.mtp_path.clone())
        } else {
            find_file_in_cache(&config.mtp_repo, "mtp")
                .or_else(|| find_file_in_cache(&config.model_repo, "mtp"))
        }
    } else {
        None
    };

    if config.model_path.is_file() && config.mmproj_path.is_file() {
        return Some((config.model_path.clone(), config.mmproj_path.clone(), mtp_found));
    }
    find_in_local_cache(&config.model_repo).map(|(m, p, cached_mtp)| (m, p, mtp_found.or(cached_mtp)))
}

struct ChannelProgress {
    file: String,
    tx: Sender<DownloadProgress>,
}

impl ProgressHandler for ChannelProgress {
    fn on_progress(&self, event: &ProgressEvent) {
        match event {
            ProgressEvent::Download(DownloadEvent::Progress { files }) => {
                for f in files {
                    let pct = if f.total_bytes > 0 {
                        (f.bytes_completed as f32 / f.total_bytes as f32) * 100.0
                    } else {
                        0.0
                    };
                    let _ = self.tx.send(DownloadProgress::Downloading {
                        file: f.filename.clone(),
                        current_bytes: f.bytes_completed,
                        total_bytes: f.total_bytes,
                        pct,
                    });
                }
            }
            ProgressEvent::Download(DownloadEvent::AggregateProgress {
                bytes_completed,
                total_bytes,
                ..
            }) => {
                let pct = if *total_bytes > 0 {
                    (*bytes_completed as f32 / *total_bytes as f32) * 100.0
                } else {
                    0.0
                };
                let _ = self.tx.send(DownloadProgress::Downloading {
                    file: self.file.clone(),
                    current_bytes: *bytes_completed,
                    total_bytes: *total_bytes,
                    pct,
                });
            }
            _ => {}
        }
    }
}

/// Spawns an asynchronous background task on Bevy's IoTaskPool to verify cache or download models via `hf-hub`.
pub fn spawn_auto_downloader(
    config: Config,
    tx: Sender<DownloadProgress>,
) {
    let pool = IoTaskPool::get_or_init(TaskPool::default);
    pool.spawn(async move {
        let _ = tx.send(DownloadProgress::CheckingCache);

        // 1. Check local files and HF cache first
        if let Some((model, mmproj, mtp)) = check_models_available(&config) {
            let _ = tx.send(DownloadProgress::CachedFound {
                model: model.clone(),
                mmproj: mmproj.clone(),
                mtp: mtp.clone(),
            });
            let _ = tx.send(DownloadProgress::Complete { model, mmproj, mtp });
            return;
        }

        // 2. Download using hf-hub via Bevy task pool
        let (owner, name) = split_id(&config.model_repo);
        let client = match hf_hub::HFClient::new() {
            Ok(c) => c,
            Err(err) => {
                let _ = tx.send(DownloadProgress::Error(format!("Failed to create HFClient: {err}")));
                return;
            }
        };
        let repo = client.model(owner, name);

        // Determine filenames from target paths or defaults
        let model_name = config
            .model_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model.gguf");
        let mmproj_name = config
            .mmproj_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("mmproj.gguf");

        // Download model
        let model_handler = std::sync::Arc::new(ChannelProgress {
            file: model_name.to_string(),
            tx: tx.clone(),
        });
        let model_cached = match repo
            .download_file()
            .filename(model_name)
            .progress(model_handler)
            .send()
            .await
        {
            Ok(path) => path,
            Err(e) => {
                let _ = tx.send(DownloadProgress::Error(format!("Download model error: {e}")));
                return;
            }
        };

        // Download mmproj
        let mmproj_handler = std::sync::Arc::new(ChannelProgress {
            file: mmproj_name.to_string(),
            tx: tx.clone(),
        });
        let mmproj_cached = match repo
            .download_file()
            .filename(mmproj_name)
            .progress(mmproj_handler)
            .send()
            .await
        {
            Ok(path) => path,
            Err(e) => {
                let _ = tx.send(DownloadProgress::Error(format!("Download mmproj error: {e}")));
                return;
            }
        };

        // Download MTP drafter if enabled and missing
        let mtp_cached = if config.enable_mtp {
            if config.mtp_path.is_file() {
                Some(config.mtp_path.clone())
            } else if let Some(p) = find_file_in_cache(&config.mtp_repo, "mtp")
                .or_else(|| find_file_in_cache(&config.model_repo, "mtp"))
            {
                Some(p)
            } else {
                let (mtp_owner, mtp_name_repo) = split_id(&config.mtp_repo);
                let mtp_repo = client.model(mtp_owner, mtp_name_repo);
                let mtp_file_name = config
                    .mtp_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(crate::pet_state::MTP_FILE);
                let mtp_handler = std::sync::Arc::new(ChannelProgress {
                    file: mtp_file_name.to_string(),
                    tx: tx.clone(),
                });
                match mtp_repo
                    .download_file()
                    .filename(mtp_file_name)
                    .progress(mtp_handler)
                    .send()
                    .await
                {
                    Ok(path) => Some(path),
                    Err(e) => {
                        eprintln!("reelbuddy: warning: optional mtp download failed: {e}");
                        None
                    }
                }
            }
        } else {
            None
        };

        let _ = tx.send(DownloadProgress::Complete {
            model: model_cached,
            mmproj: mmproj_cached,
            mtp: mtp_cached,
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_checker_does_not_panic_on_missing_repo() {
        let res = find_in_local_cache("nonexistent/test-model-repo-12345");
        assert!(res.is_none());
    }
}
