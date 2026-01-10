use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
	pub theme: String,
	pub language: String,
	pub cluster_path: Option<PathBuf>,
	pub window_width: u32,
	pub window_height: u32,
	pub download_concurrency: u8,
	pub game: GameDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GameDefaults {
	pub java_path: Option<PathBuf>,
	pub max_memory_mb: u32,
	pub window_width: u32,
	pub window_height: u32,
	pub jvm_args: String,
}

impl Default for LauncherConfig {
	fn default() -> Self {
		Self {
			theme: "dark".into(),
			language: "zh-CN".into(),
			cluster_path: None,
			window_width: 900,
			window_height: 550,
			download_concurrency: 5,
			game: GameDefaults::default(),
		}
	}
}

impl Default for GameDefaults {
	fn default() -> Self {
		Self {
			java_path: None,
			max_memory_mb: 4096,
			window_width: 854,
			window_height: 480,
			jvm_args: String::new(),
		}
	}
}
