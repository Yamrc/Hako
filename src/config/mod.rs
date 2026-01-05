pub mod game;
pub mod launcher;
pub mod manager;

pub use game::{GameConfig, ResolvedGameConfig};
pub use launcher::{GameDefaults, LauncherConfig};
pub use manager::{
	ConfigManager, GameConfigDiff, GameDefaultsDiff, LauncherConfigDiff,
};
