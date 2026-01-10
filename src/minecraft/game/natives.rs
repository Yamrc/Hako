use crate::minecraft::game::args::{Features, current_arch, current_os_key};
use crate::minecraft::game::classpath::{library_applicable, library_path};
use crate::minecraft::profile::VersionProfile;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub fn get_natives_directory(game_dir: &Path, version: &str) -> Result<PathBuf> {
	let default_dir = game_dir.join("versions").join(version).join("natives");
	let path_str = default_dir.to_string_lossy();
	if path_str.chars().all(|c| c.is_ascii()) {
		return Ok(default_dir);
	}

	#[cfg(windows)]
	{
		if let Some(home) = dirs::home_dir() {
			let fallback = home.join(".minecraft").join("bin").join("natives");
			if fallback.to_string_lossy().chars().all(|c| c.is_ascii()) {
				tracing::info!("Using fallback natives directory: {}", fallback.display());
				return Ok(fallback);
			}
		}

		if let Ok(pd) = std::env::var("ProgramData") {
			let pd_path = PathBuf::from(pd).join("Hako").join("natives");
			if pd_path.to_string_lossy().chars().all(|c| c.is_ascii()) {
				tracing::info!("Using ProgramData natives directory: {}", pd_path.display());
				return Ok(pd_path);
			}
		}
	}

	tracing::warn!(
		"Could not find ASCII natives directory, using default: {}",
		default_dir.display()
	);
	Ok(default_dir)
}

pub fn extract_natives(
	game_dir: &Path,
	profile: &VersionProfile,
	natives_dir: &Path,
	features: &Features,
) -> Result<()> {
	fs::create_dir_all(natives_dir).context("Failed to create natives directory")?;

	let os_key = current_os_key();
	let arch = current_arch();
	let mut extracted = HashSet::new();

	for lib in &profile.libraries {
		if lib.natives.is_none() || !library_applicable(lib, os_key, arch, features) {
			continue;
		}

		let natives_jar = match library_path(game_dir, lib, os_key, arch)? {
			Some(p) if p.exists() => p,
			Some(p) => {
				tracing::warn!("Natives jar missing: {}, skipping", p.display());
				continue;
			}
			None => continue,
		};

		tracing::info!("Extracting natives from: {}", natives_jar.display());
		let file = std::fs::File::open(&natives_jar)
			.with_context(|| format!("Failed to open: {}", natives_jar.display()))?;
		let mut archive = ZipArchive::new(file)
			.with_context(|| format!("Failed to read zip: {}", natives_jar.display()))?;

		let exclude: Vec<&str> = lib
			.extract
			.as_ref()
			.map(|e| e.exclude.iter().map(|s| s.as_str()).collect())
			.unwrap_or_default();

		for i in 0..archive.len() {
			let mut file = archive.by_index(i).context("Failed to read zip entry")?;
			let name = file.name().to_string();

			if exclude
				.iter()
				.any(|p| name.starts_with(p) || name.contains(p))
			{
				continue;
			}

			if !name.ends_with(".dll")
				&& !name.ends_with(".so")
				&& !name.ends_with(".dylib")
				&& !name.ends_with(".jnilib")
			{
				continue;
			}

			let target = natives_dir.join(&name);

			if target.exists() {
				if let Ok(m) = target.metadata() {
					if m.len() == file.size() {
						extracted.insert(target.clone());
						continue;
					}
				}
				let _ = fs::remove_file(&target);
			}

			if let Some(parent) = target.parent() {
				fs::create_dir_all(parent)
					.with_context(|| format!("Failed to create dir: {}", parent.display()))?;
			}

			std::io::copy(
				&mut file,
				&mut std::fs::File::create(&target)
					.with_context(|| format!("Failed to create: {}", target.display()))?,
			)
			.with_context(|| format!("Failed to write: {}", target.display()))?;

			extracted.insert(target);
		}
	}

	if let Ok(entries) = fs::read_dir(natives_dir) {
		for entry in entries.flatten() {
			let path = entry.path();
			if path.is_file() && !extracted.contains(&path) {
				let _ = fs::remove_file(&path);
			}
		}
	}

	Ok(())
}
