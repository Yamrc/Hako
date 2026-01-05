use crate::domain::game::profile::{ArgValueInner, ArgumentValue, Rule, RuleOs, VersionProfile};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

static TEMPLATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$\{([^}]+)\}").unwrap());

#[derive(Debug, Clone, Default)]
pub struct Features {
	#[allow(dead_code)]
	pub is_demo_user: bool,
	#[allow(dead_code)]
	pub has_custom_resolution: bool,
	#[allow(dead_code)]
	pub has_quick_plays_support: bool,
	#[allow(dead_code)]
	pub is_quick_play_singleplayer: bool,
	#[allow(dead_code)]
	pub is_quick_play_multiplayer: bool,
	#[allow(dead_code)]
	pub is_quick_play_realms: bool,
}

pub fn collect_jvm_args(
	profile: &VersionProfile,
	game_dir: &Path,
	version: &str,
	classpath: &str,
	assets_index: &str,
	username: &str,
	uuid: &str,
	natives_dir: &Path,
	features: &Features,
) -> Vec<String> {
	let mut replacements = build_replacements(
		game_dir,
		version,
		assets_index,
		username,
		uuid,
		Some(natives_dir),
		Some(classpath),
	);
	replacements.insert("${launcher_name}".to_string(), "Hako".to_string());
	replacements.insert(
		"${launcher_version}".to_string(),
		env!("CARGO_PKG_VERSION").to_string(),
	);
	replacements.insert(
		"${library_directory}".to_string(),
		game_dir.join("libraries").to_string_lossy().into_owned(),
	);
	replacements.insert(
		"${classpath_separator}".to_string(),
		if cfg!(windows) { ";" } else { ":" }.to_string(),
	);
	collect_args(profile, true, &replacements, features)
}

pub fn collect_game_args(
	game_dir: &Path,
	version: &str,
	profile: &VersionProfile,
	username: &str,
	uuid: &str,
	assets_index: &str,
	features: &Features,
) -> Vec<String> {
	let mut replacements =
		build_replacements(game_dir, version, assets_index, username, uuid, None, None);
	replacements.insert("${version}".to_string(), version.to_string());
	replacements.insert("${assetIndex}".to_string(), assets_index.to_string());
	replacements.insert("${accessToken}".to_string(), "0".to_string());
	replacements.insert("${userType}".to_string(), "mojang".to_string());

	if profile.arguments.is_some() {
		collect_args(profile, false, &replacements, features)
	} else if let Some(legacy) = &profile.minecraft_arguments {
		let assets_dir = game_dir.join("assets");
		let mut out: Vec<String> = legacy
			.split_whitespace()
			.flat_map(|s| replace_and_split(s, &replacements))
			.collect();
		out.extend([
			"--username".into(),
			username.into(),
			"--uuid".into(),
			uuid.into(),
			"--version".into(),
			version.into(),
			"--gameDir".into(),
			game_dir.to_string_lossy().into_owned(),
			"--assetsDir".into(),
			assets_dir.to_string_lossy().into_owned(),
			"--assetIndex".into(),
			assets_index.into(),
			"--accessToken".into(),
			"0".into(),
			"--userType".into(),
			"mojang".into(),
		]);
		out
	} else {
		Vec::new()
	}
}

fn build_replacements(
	game_dir: &Path,
	version: &str,
	assets_index: &str,
	username: &str,
	uuid: &str,
	natives_dir: Option<&Path>,
	classpath: Option<&str>,
) -> HashMap<String, String> {
	let assets_dir = game_dir.join("assets");
	let mut replacements = HashMap::new();

	replacements.insert("${version_name}".to_string(), version.to_string());
	replacements.insert("${username}".to_string(), username.to_string());
	replacements.insert("${auth_player_name}".to_string(), username.to_string());
	replacements.insert("${uuid}".to_string(), uuid.to_string());
	replacements.insert("${auth_uuid}".to_string(), uuid.to_string());
	replacements.insert(
		"${gameDir}".to_string(),
		game_dir.to_string_lossy().into_owned(),
	);
	replacements.insert(
		"${game_directory}".to_string(),
		game_dir.to_string_lossy().into_owned(),
	);
	replacements.insert(
		"${assetsDir}".to_string(),
		assets_dir.to_string_lossy().into_owned(),
	);
	replacements.insert(
		"${assets_root}".to_string(),
		assets_dir.to_string_lossy().into_owned(),
	);
	replacements.insert(
		"${game_assets}".to_string(),
		assets_dir.to_string_lossy().into_owned(),
	);
	replacements.insert("${assetIndex}".to_string(), assets_index.to_string());
	replacements.insert("${assets_index_name}".to_string(), assets_index.to_string());
	replacements.insert("${auth_access_token}".to_string(), "0".to_string());
	replacements.insert("${auth_session}".to_string(), "0".to_string());
	replacements.insert("${user_type}".to_string(), "mojang".to_string());

	if let Some(natives_dir) = natives_dir {
		replacements.insert(
			"${natives_directory}".to_string(),
			natives_dir.to_string_lossy().into_owned(),
		);
	}
	if let Some(classpath) = classpath {
		replacements.insert("${classpath}".to_string(), classpath.to_string());
	}

	replacements
}

fn collect_args(
	profile: &VersionProfile,
	is_jvm: bool,
	replacements: &HashMap<String, String>,
	features: &Features,
) -> Vec<String> {
	if let Some(args) = &profile.arguments {
		let values = if is_jvm { &args.jvm } else { &args.game };
		expand_args(values, replacements, features)
	} else {
		Vec::new()
	}
}

pub fn current_os_key() -> &'static str {
	if cfg!(target_os = "windows") {
		"windows"
	} else if cfg!(target_os = "macos") {
		"osx"
	} else {
		"linux"
	}
}

pub fn current_arch() -> &'static str {
	if cfg!(target_pointer_width = "64") {
		"x86_64"
	} else {
		"x86"
	}
}

fn replace_and_split(s: &str, replacements: &HashMap<String, String>) -> Vec<String> {
	TEMPLATE_RE
		.replace_all(s, |caps: &regex::Captures| {
			replacements
				.get(caps.get(0).unwrap().as_str())
				.cloned()
				.unwrap_or_else(|| caps.get(0).unwrap().as_str().to_string())
		})
		.split_whitespace()
		.map(String::from)
		.collect()
}

pub fn expand_args(
	values: &[ArgumentValue],
	replacements: &HashMap<String, String>,
	features: &Features,
) -> Vec<String> {
	let os_key = current_os_key();
	let arch = current_arch();
	let mut out = Vec::new();

	for v in values {
		match v {
			ArgumentValue::Plain(s) => {
				out.extend(replace_and_split(s, replacements));
			}
			ArgumentValue::Obj(o) => {
				if rule_allows(o.rules.as_ref(), os_key, arch, features) {
					match &o.value {
						ArgValueInner::One(s) => {
							out.extend(replace_and_split(s, replacements));
						}
						ArgValueInner::Many(list) => {
							for s in list {
								out.extend(replace_and_split(s, replacements));
							}
						}
					}
				}
			}
		}
	}

	out
}

pub fn rule_allows(
	rules: Option<&Vec<Rule>>,
	os_key: &str,
	arch: &str,
	features: &Features,
) -> bool {
	let Some(rules) = rules else {
		return true;
	};
	let mut allow = false;
	for rule in rules {
		if os_rule_match(rule.os.as_ref(), os_key, arch)
			&& features_match(rule.features.as_ref(), features)
		{
			allow = rule.action == "allow";
		}
	}
	allow
}

fn features_match(features: Option<&HashMap<String, bool>>, current: &Features) -> bool {
	let Some(map) = features else {
		return true;
	};
	for (key, required) in map {
		let current_value = match key.as_str() {
			"is_demo_user" => current.is_demo_user,
			"has_custom_resolution" => current.has_custom_resolution,
			"has_quick_plays_support" => current.has_quick_plays_support,
			"is_quick_play_singleplayer" => current.is_quick_play_singleplayer,
			"is_quick_play_multiplayer" => current.is_quick_play_multiplayer,
			"is_quick_play_realms" => current.is_quick_play_realms,
			_ => false,
		};
		if *required != current_value {
			return false;
		}
	}
	true
}

fn os_rule_match(os: Option<&RuleOs>, os_key: &str, arch: &str) -> bool {
	let Some(o) = os else {
		return true;
	};
	let name_ok = o.name.as_deref().map_or(true, |n| n == os_key);
	let arch_ok = o.arch.as_deref().map_or(true, |a| a == arch);
	let version_ok = if let Some(pattern) = &o.version {
		get_os_version().map_or(false, |v| {
			Regex::new(pattern).map_or_else(
				|e| {
					tracing::warn!("Invalid OS version regex '{}': {}", pattern, e);
					false
				},
				|re| re.is_match(&v),
			)
		})
	} else {
		true
	};
	name_ok && arch_ok && version_ok
}

fn get_os_version() -> Option<String> {
	#[cfg(windows)]
	{
		std::process::Command::new("cmd")
			.args(["/C", "ver"])
			.output()
			.ok()
			.and_then(|o| String::from_utf8(o.stdout).ok())
			.and_then(|s| {
				s.find("Version ").map(|i| {
					s[i + 8..]
						.trim()
						.split('\n')
						.next()
						.unwrap_or("")
						.trim()
						.to_string()
				})
			})
	}
	#[cfg(target_os = "macos")]
	{
		std::process::Command::new("sw_vers")
			.arg("-productVersion")
			.output()
			.ok()
			.and_then(|o| String::from_utf8(o.stdout).ok())
			.map(|s| s.trim().to_string())
	}
	#[cfg(target_os = "linux")]
	{
		std::process::Command::new("uname")
			.arg("-r")
			.output()
			.ok()
			.and_then(|o| String::from_utf8(o.stdout).ok())
			.map(|s| s.trim().to_string())
	}
	#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
	{
		None
	}
}
