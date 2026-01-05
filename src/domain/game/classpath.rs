use crate::domain::game::args::{Features, current_arch, current_os_key, rule_allows};
use crate::domain::game::profile::{Library, VersionProfile};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn build_classpath(
	game_dir: &Path,
	version: &str,
	profile: &VersionProfile,
	features: &Features,
) -> Result<String> {
	let version_jar = game_dir
		.join("versions")
		.join(version)
		.join(format!("{version}.jar"));
	if !version_jar.exists() {
		return Err(anyhow::anyhow!(
			"Version jar missing: {}",
			version_jar.display()
		));
	}

	let os_key = current_os_key();
	let arch = current_arch();
	let mut seen = HashSet::new();
	let mut paths = Vec::new();

	for lib in &profile.libraries {
		if !library_applicable(lib, os_key, arch, features) {
			continue;
		}

		if let Some(p) = library_path(game_dir, lib, os_key, arch)? {
			if !p.exists() {
				return Err(anyhow::anyhow!("Library missing: {}", p.display()));
			}
			if seen.insert(p.clone()) {
				paths.push(p);
			}
		}
	}

	paths.push(version_jar);
	let sep = if cfg!(windows) { ";" } else { ":" };
	Ok(paths
		.iter()
		.map(|p| p.to_string_lossy().into_owned())
		.collect::<Vec<_>>()
		.join(sep))
}

fn maven_path(game_dir: &Path, coord: &str, classifier: Option<&str>) -> Result<PathBuf> {
	let parts: Vec<&str> = coord.split(':').collect();
	if parts.len() < 3 {
		return Err(anyhow::anyhow!("Invalid maven coord: {coord}"));
	}
	let group = parts[0].replace('.', "/");
	let artifact = parts[1];
	let version = parts[2];
	let classifier = classifier.or_else(|| parts.get(3).copied());

	let file_name = if let Some(c) = classifier {
		format!("{artifact}-{version}-{c}.jar")
	} else {
		format!("{artifact}-{version}.jar")
	};

	Ok(game_dir
		.join("libraries")
		.join(group)
		.join(artifact)
		.join(version)
		.join(file_name))
}

pub fn library_applicable(lib: &Library, os_key: &str, arch: &str, features: &Features) -> bool {
	rule_allows(lib.rules.as_ref(), os_key, arch, features)
}

pub fn library_path(
	game_dir: &Path,
	lib: &Library,
	os_key: &str,
	arch: &str,
) -> Result<Option<PathBuf>> {
	if let Some(natives) = &lib.natives {
		if let Some(native_cls) = natives.get(os_key) {
			let classifier = native_cls.replace("${arch}", arch);
			if let Some(dl) = &lib.downloads {
				if let Some(classifiers) = &dl.classifiers {
					let key = format!("natives-{os_key}");
					if let Some(artifact) = classifiers.get(&key) {
						if let Some(path) = &artifact.path {
							return Ok(Some(
								game_dir
									.join("libraries")
									.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)),
							));
						}
					}
				}
			}
			return Ok(Some(maven_path(game_dir, &lib.name, Some(&classifier))?));
		}
		return Ok(None);
	}

	if let Some(dl) = &lib.downloads {
		if let Some(artifact) = &dl.artifact {
			if let Some(path) = &artifact.path {
				return Ok(Some(
					game_dir
						.join("libraries")
						.join(path.replace('/', std::path::MAIN_SEPARATOR_STR)),
				));
			}
		}
	}

	Ok(Some(maven_path(game_dir, &lib.name, None)?))
}
