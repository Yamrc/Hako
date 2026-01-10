use crate::config::game::GameConfig;
use crate::config::launcher::LauncherConfig;
use crate::core::paths;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::sync::RwLock;

pub struct ConfigManager {
	config: RwLock<LauncherConfig>,
}

impl ConfigManager {
	pub fn new() -> Result<Self> {
		let config = Self::load_from_disk()?;
		Ok(Self {
			config: RwLock::new(config),
		})
	}

	fn load_from_disk() -> Result<LauncherConfig> {
		let config_path = paths::config_dir()?.join("config.yml");
		if !config_path.exists() {
			return Ok(LauncherConfig::default());
		}
		let content = fs::read_to_string(&config_path).context("read config")?;
		serde_yaml::from_str(&content).context("parse config")
	}

	pub fn get(&self) -> LauncherConfig {
		self.config.read().unwrap().clone()
	}

	pub fn update<F>(&self, f: F) -> Result<()>
	where
		F: FnOnce(&mut LauncherConfig),
	{
		let mut config = self.config.write().unwrap();
		f(&mut config);
		self.save_to_disk(&config)
	}

	fn save_to_disk(&self, config: &LauncherConfig) -> Result<()> {
		let config_dir = paths::config_dir()?;
		fs::create_dir_all(&config_dir)?;
		let yaml = serde_yaml::to_string(config)?;
		fs::write(config_dir.join("config.yml"), yaml)?;
		Ok(())
	}

	pub fn reload(&self) -> Result<()> {
		let new_config = Self::load_from_disk()?;
		*self.config.write().unwrap() = new_config;
		Ok(())
	}

	// 游戏实例配置
	pub fn load_game_config(cluster_path: &Path, version: &str) -> GameConfig {
		let path = cluster_path
			.join("versions")
			.join(version)
			.join("Hako")
			.join("settings.yml");

		if !path.exists() {
			return GameConfig::default();
		}

		fs::read_to_string(&path)
			.ok()
			.and_then(|s| serde_yaml::from_str(&s).ok())
			.unwrap_or_default()
	}

	pub fn save_game_config(cluster_path: &Path, version: &str, config: &GameConfig) -> Result<()> {
		let dir = cluster_path.join("versions").join(version).join("Hako");
		fs::create_dir_all(&dir)?;
		let yaml = serde_yaml::to_string(config)?;
		fs::write(dir.join("settings.yml"), yaml)?;
		Ok(())
	}
}

impl Default for ConfigManager {
	fn default() -> Self {
		Self::new().unwrap_or_else(|_| Self {
			config: RwLock::new(LauncherConfig::default()),
		})
	}
}
