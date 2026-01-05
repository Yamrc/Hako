use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GameConfig {
	pub java_path: Option<PathBuf>,
	pub max_memory_mb: Option<u32>,
	pub window_width: Option<u32>,
	pub window_height: Option<u32>,
	pub jvm_args: Option<String>,
	pub game_args: Option<String>,
}

impl GameConfig {
	pub fn resolve(&self, defaults: &super::launcher::GameDefaults) -> ResolvedGameConfig {
		ResolvedGameConfig {
			java_path: self
				.java_path
				.clone()
				.or_else(|| defaults.java_path.clone()),
			max_memory_mb: self.max_memory_mb.unwrap_or(defaults.max_memory_mb),
			window_width: self.window_width.unwrap_or(defaults.window_width),
			window_height: self.window_height.unwrap_or(defaults.window_height),
			jvm_args: self
				.jvm_args
				.clone()
				.unwrap_or_else(|| defaults.jvm_args.clone()),
			game_args: self.game_args.clone().unwrap_or_default(),
		}
	}
}

#[derive(Debug, Clone)]
pub struct ResolvedGameConfig {
	pub java_path: Option<PathBuf>,
	pub max_memory_mb: u32,
	pub window_width: u32,
	pub window_height: u32,
	pub jvm_args: String,
	pub game_args: String,
}
