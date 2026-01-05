use crate::core::paths;
use crate::infrastructure::config::game::GameConfig;
use crate::infrastructure::config::launcher::LauncherConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LauncherConfigDiff {
	pub theme: Option<String>,
	pub language: Option<String>,
	pub cluster_path: Option<PathBuf>,
	pub window_width: Option<u32>,
	pub window_height: Option<u32>,
	pub download_concurrency: Option<u8>,
	pub game: Option<GameDefaultsDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GameDefaultsDiff {
	pub java_path: Option<PathBuf>,
	pub max_memory_mb: Option<u32>,
	pub window_width: Option<u32>,
	pub window_height: Option<u32>,
	pub jvm_args: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GameConfigDiff {
	pub java_path: Option<PathBuf>,
	pub max_memory_mb: Option<u32>,
	pub window_width: Option<u32>,
	pub window_height: Option<u32>,
	pub jvm_args: Option<String>,
	pub game_args: Option<String>,
}

#[derive(Debug)]
pub struct ConfigManager {
	config_path: PathBuf,
	config: RwLock<LauncherConfig>,
}

impl ConfigManager {
	pub async fn new() -> Result<Self> {
		let config_dir = paths::config_dir()?;
		tokio::fs::create_dir_all(&config_dir).await?;
		let config_path = config_dir.join("config.yml");

		let config = if config_path.exists() {
			Self::load_from_disk(&config_path).await?
		} else {
			let default = LauncherConfig::default();
			Self::save_to_disk(&config_path, &default).await?;
			default
		};

		Ok(Self {
			config_path,
			config: RwLock::new(config),
		})
	}

	async fn load_from_disk(path: &Path) -> Result<LauncherConfig> {
		let content = tokio::fs::read_to_string(path)
			.await
			.context("read config")?;
		serde_yaml::from_str(&content).context("parse config")
	}

	async fn save_to_disk(path: &Path, config: &LauncherConfig) -> Result<()> {
		let yaml = serde_yaml::to_string(config)?;
		tokio::fs::write(path, yaml).await?;
		Ok(())
	}

	pub async fn get(&self) -> LauncherConfig {
		self.config.read().await.clone()
	}

	pub fn get_sync(&self) -> LauncherConfig {
		tokio::task::block_in_place(|| {
			tokio::runtime::Handle::current()
				.block_on(self.config.read())
		})
		.clone()
	}

	pub async fn update(&self, diff: LauncherConfigDiff) -> Result<()> {
		let mut config = self.config.write().await;

		if let Some(theme) = diff.theme {
			config.theme = theme;
		}
		if let Some(language) = diff.language {
			config.language = language;
		}
		if let Some(cluster_path) = diff.cluster_path {
			config.cluster_path = Some(cluster_path);
		}
		if let Some(window_width) = diff.window_width {
			config.window_width = window_width;
		}
		if let Some(window_height) = diff.window_height {
			config.window_height = window_height;
		}
		if let Some(download_concurrency) = diff.download_concurrency {
			config.download_concurrency = download_concurrency;
		}
		if let Some(game_diff) = diff.game {
			if let Some(java_path) = game_diff.java_path {
				config.game.java_path = Some(java_path);
			}
			if let Some(max_memory_mb) = game_diff.max_memory_mb {
				config.game.max_memory_mb = max_memory_mb;
			}
			if let Some(window_width) = game_diff.window_width {
				config.game.window_width = window_width;
			}
			if let Some(window_height) = game_diff.window_height {
				config.game.window_height = window_height;
			}
			if let Some(jvm_args) = game_diff.jvm_args {
				config.game.jvm_args = jvm_args;
			}
		}

		Self::save_to_disk(&self.config_path, &config).await?;
		Ok(())
	}

	pub async fn reload(&self) -> Result<()> {
		let new_config = Self::load_from_disk(&self.config_path).await?;
		*self.config.write().await = new_config;
		Ok(())
	}

	pub async fn get_game_config(&self, cluster_path: &Path, version: &str) -> GameConfig {
		let path = cluster_path
			.join("versions")
			.join(version)
			.join("Hako")
			.join("settings.yml");

		if !path.exists() {
			return GameConfig::default();
		}

		tokio::fs::read_to_string(&path)
			.await
			.ok()
			.and_then(|s| serde_yaml::from_str(&s).ok())
			.unwrap_or_default()
	}

	pub async fn update_game_config(
		&self,
		cluster_path: &Path,
		version: &str,
		diff: GameConfigDiff,
	) -> Result<()> {
		let dir = cluster_path.join("versions").join(version).join("Hako");
		tokio::fs::create_dir_all(&dir).await?;

		let path = dir.join("settings.yml");
		let mut config = if path.exists() {
			let content = tokio::fs::read_to_string(&path).await?;
			serde_yaml::from_str(&content).unwrap_or_default()
		} else {
			GameConfig::default()
		};

		if let Some(java_path) = diff.java_path {
			config.java_path = Some(java_path);
		}
		if let Some(max_memory_mb) = diff.max_memory_mb {
			config.max_memory_mb = Some(max_memory_mb);
		}
		if let Some(window_width) = diff.window_width {
			config.window_width = Some(window_width);
		}
		if let Some(window_height) = diff.window_height {
			config.window_height = Some(window_height);
		}
		if let Some(jvm_args) = diff.jvm_args {
			config.jvm_args = Some(jvm_args);
		}
		if let Some(game_args) = diff.game_args {
			config.game_args = Some(game_args);
		}

		let yaml = serde_yaml::to_string(&config)?;
		tokio::fs::write(path, yaml).await?;
		Ok(())
	}

	pub async fn validate(&self) -> Result<()> {
		let config = self.config.read().await;

		if config.download_concurrency == 0 {
			return Err(anyhow::anyhow!("download_concurrency must be > 0"));
		}

		if config.window_width < 800 || config.window_width > 3840 {
			return Err(anyhow::anyhow!("window_width must be between 800 and 3840"));
		}

		if config.window_height < 600 || config.window_height > 2160 {
			return Err(anyhow::anyhow!(
				"window_height must be between 600 and 2160"
			));
		}

		if config.game.max_memory_mb < 512 || config.game.max_memory_mb > 65536 {
			return Err(anyhow::anyhow!(
				"max_memory_mb must be between 512 and 65536"
			));
		}

		Ok(())
	}

	pub async fn reset_to_default(&self) -> Result<()> {
		let default = LauncherConfig::default();
		*self.config.write().await = default.clone();
		Self::save_to_disk(&self.config_path, &default).await?;
		Ok(())
	}

	pub async fn export(&self) -> Result<String> {
		let config = self.config.read().await;
		serde_yaml::to_string(&*config).context("export config")
	}

	pub async fn import(&self, yaml: &str) -> Result<()> {
		let config: LauncherConfig = serde_yaml::from_str(yaml).context("import config")?;
		*self.config.write().await = config.clone();
		Self::save_to_disk(&self.config_path, &config).await?;
		Ok(())
	}
}

impl Default for ConfigManager {
	fn default() -> Self {
		let config_dir =
			paths::config_dir().unwrap_or_else(|_| std::env::current_dir().unwrap().join(".hako"));
		let config_path = config_dir.join("config.yml");

		Self {
			config_path,
			config: RwLock::new(LauncherConfig::default()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_config_diff_update() {
		let manager = ConfigManager::default();
		let diff = LauncherConfigDiff {
			theme: Some("light".into()),
			language: Some("en-US".into()),
			..Default::default()
		};

		manager.update(diff).await.unwrap();

		let config = manager.get().await;
		assert_eq!(config.theme, "light");
		assert_eq!(config.language, "en-US");
	}

	#[tokio::test]
	async fn test_game_config_diff() {
		let manager = ConfigManager::default();
		let diff = GameConfigDiff {
			max_memory_mb: Some(8192),
			jvm_args: Some("-Xms512M".into()),
			..Default::default()
		};

		manager
			.update_game_config(Path::new("/tmp/test"), "1.20.1", diff)
			.await
			.unwrap();

		let config = manager
			.get_game_config(Path::new("/tmp/test"), "1.20.1")
			.await;
		assert_eq!(config.max_memory_mb, Some(8192));
		assert_eq!(config.jvm_args, Some("-Xms512M".into()));
	}
}
