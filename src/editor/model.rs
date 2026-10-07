//! Phase 0 only resolves GGUF paths. The weights are not loaded.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pet_state::{self, PetSnapshot};

fn default_true() -> bool {
    true
}

fn default_mtp_repo() -> String {
    pet_state::MTP_REPO.to_string()
}

fn default_empty_path() -> PathBuf {
    PathBuf::new()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub model_repo: String,
    pub model_path: PathBuf,
    pub mmproj_path: PathBuf,
    #[serde(default = "default_true")]
    pub enable_mtp: bool,
    #[serde(default = "default_mtp_repo")]
    pub mtp_repo: String,
    #[serde(default = "default_empty_path")]
    pub mtp_path: PathBuf,
}

impl Config {
    pub fn default_in(app_data: &Path) -> Self {
        let models = app_data.join("models");
        Self {
            model_repo: pet_state::MODEL_REPO.to_string(),
            model_path: models.join("model.gguf"),
            mmproj_path: models.join("mmproj.gguf"),
            enable_mtp: true,
            mtp_repo: pet_state::MTP_REPO.to_string(),
            mtp_path: models.join(pet_state::MTP_FILE),
        }
    }
}

pub fn app_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "reelbuddy", "reelbuddy")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn load_or_create(app_data: &Path) -> Result<Config, String> {
    std::fs::create_dir_all(app_data).map_err(|err| {
        pet_state::model_error_line(&format!("Asleep. Could not create app data: {err}"))
    })?;
    let path = app_data.join("config.toml");
    if !path.exists() {
        let config = Config::default_in(app_data);
        let body = toml::to_string_pretty(&config)
            .map_err(|err| pet_state::model_error_line(&format!("Asleep. Bad config: {err}")))?;
        let text = format!(
            "# huggingface: {}\n{body}",
            pet_state::MODEL_REPO
        );
        std::fs::write(&path, text).map_err(|err| {
            pet_state::model_error_line(&format!("Asleep. Could not write config: {err}"))
        })?;
        return Ok(config);
    }
    let text = std::fs::read_to_string(&path).map_err(|err| {
        pet_state::model_error_line(&format!("Asleep. Could not read config: {err}"))
    })?;
    let mut config: Config = toml::from_str(&text)
        .map_err(|err| pet_state::model_error_line(&format!("Asleep. Could not parse config: {err}")))?;
    if config.mtp_path.as_os_str().is_empty() {
        config.mtp_path = app_data.join("models").join(pet_state::MTP_FILE);
    }
    if config.mtp_repo.is_empty() {
        config.mtp_repo = pet_state::MTP_REPO.to_string();
    }
    Ok(config)
}

/// Files on disk count as present. Phase 0 does not open the GGUF.
pub fn snapshot_for_config(config: &Config) -> PetSnapshot {
    let model_ok = config.model_path.is_file();
    let proj_ok = config.mmproj_path.is_file();
    if model_ok && proj_ok {
        PetSnapshot {
            line: pet_state::seed_line(),
            model_present: true,
            woke: false,
            ..PetSnapshot::seed_asleep(String::new())
        }
    } else {
        PetSnapshot::seed_asleep(pet_state::missing_model_line())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_stay_asleep() {
        let dir = std::env::temp_dir().join(format!(
            "reelbuddy-phase0-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let config = load_or_create(&dir).unwrap();
        assert_eq!(config.model_repo, pet_state::MODEL_REPO);
        assert!(!config.model_path.exists());
        let pet = snapshot_for_config(&config);
        assert!(!pet.model_present);
        assert_eq!(pet.sprite(), crate::pet_state::SpriteKind::Asleep);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
