use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GameInstance {
	pub cluster_path: PathBuf,
	pub version: String,
	pub version_path: PathBuf,
}

pub struct InstanceScanner;

impl InstanceScanner {
	pub fn scan_cluster(cluster_path: &Path) -> Result<Vec<GameInstance>> {
		let versions_dir = cluster_path.join("versions");

		if !versions_dir.exists() {
			return Ok(vec![]);
		}

		let mut instances = Vec::new();

		let entries = fs::read_dir(&versions_dir).context("Failed to read versions directory")?;

		for entry in entries {
			let entry = entry.context("Failed to read directory entry")?;
			let path = entry.path();

			if !path.is_dir() {
				continue;
			}

			let version_name = path
				.file_name()
				.and_then(|n| n.to_str())
				.map(|s| s.to_string());

			if let Some(version) = version_name {
				let json_path = path.join(format!("{}.json", version));
				if json_path.exists() {
					instances.push(GameInstance {
						cluster_path: cluster_path.to_path_buf(),
						version,
						version_path: path,
					});
				}
			}
		}

		Ok(instances)
	}

	pub fn scan_clusters(cluster_paths: &[PathBuf]) -> Result<Vec<GameInstance>> {
		let mut all_instances = Vec::new();

		for cluster_path in cluster_paths {
			match Self::scan_cluster(cluster_path) {
				Ok(instances) => all_instances.extend(instances),
				Err(e) => {
					tracing::warn!("Failed to scan cluster: {} - {}", cluster_path.display(), e);
				}
			}
		}

		Ok(all_instances)
	}
}
