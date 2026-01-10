use anyhow::Result;
use std::path::PathBuf;

// TODO: 查找多个Java实例
pub fn find_java(prefer: Option<PathBuf>) -> Result<PathBuf> {
	if let Some(p) = prefer {
		if p.exists() {
			return Ok(p);
		}
	}

	if let Ok(home) = std::env::var("JAVA_HOME") {
		let candidate =
			PathBuf::from(home)
				.join("bin")
				.join(if cfg!(windows) { "java.exe" } else { "java" });
		if candidate.exists() {
			return Ok(candidate);
		}
	}

	if let Some(paths) = std::env::var_os("PATH") {
		let name = if cfg!(windows) { "java.exe" } else { "java" };
		for p in std::env::split_paths(&paths) {
			let candidate = p.join(name);
			if candidate.exists() {
				return Ok(candidate);
			}
		}
	}

	Err(anyhow::anyhow!("Java runtime not found"))
}
