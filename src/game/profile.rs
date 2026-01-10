use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct VersionProfile {
	#[serde(default, rename = "inheritsFrom")]
	pub inherits_from: Option<String>,
	#[serde(default, rename = "mainClass")]
	pub main_class: Option<String>,
	#[serde(default)]
	pub arguments: Option<Arguments>,
	#[serde(default, rename = "minecraftArguments")]
	pub minecraft_arguments: Option<String>,
	#[serde(default)]
	pub libraries: Vec<Library>,
	#[serde(default)]
	pub assets: Option<String>,
	#[serde(default, rename = "assetIndex")]
	pub asset_index: Option<AssetIndexInfo>,
	#[serde(default)]
	pub downloads: Option<VersionDownloads>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Arguments {
	#[serde(default)]
	pub game: Vec<ArgumentValue>,
	#[serde(default)]
	pub jvm: Vec<ArgumentValue>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ArgumentValue {
	Plain(String),
	Obj(ArgObj),
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArgObj {
	#[serde(default)]
	pub rules: Option<Vec<Rule>>,
	#[serde(default)]
	pub value: ArgValueInner,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ArgValueInner {
	One(String),
	Many(Vec<String>),
}

impl Default for ArgValueInner {
	fn default() -> Self {
		ArgValueInner::One(String::new())
	}
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Library {
	pub name: String,
	#[serde(default)]
	pub natives: Option<HashMap<String, String>>,
	#[serde(default)]
	pub rules: Option<Vec<Rule>>,
	#[serde(default)]
	pub downloads: Option<LibraryDownloads>,
	#[serde(default)]
	pub extract: Option<Extract>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Extract {
	#[serde(default)]
	pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
	pub action: String,
	#[serde(default)]
	pub os: Option<RuleOs>,
	#[serde(default)]
	pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RuleOs {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub arch: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct LibraryDownloads {
	#[serde(default)]
	pub artifact: Option<Artifact>,
	#[serde(default)]
	pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Artifact {
	#[serde(default)]
	pub path: Option<String>,
	#[serde(default)]
	pub url: Option<String>,
	#[serde(default)]
	pub sha1: Option<String>,
	#[serde(default)]
	pub size: Option<u64>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AssetIndexInfo {
	#[serde(default)]
	pub id: Option<String>,
	#[serde(default)]
	pub sha1: Option<String>,
	#[serde(default)]
	pub size: Option<u64>,
	#[serde(default, rename = "totalSize")]
	pub total_size: Option<u64>,
	#[serde(default)]
	pub url: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct VersionDownloads {
	#[serde(default)]
	pub client: Option<DownloadEntry>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct DownloadEntry {
	#[serde(default)]
	pub sha1: Option<String>,
	#[serde(default)]
	pub size: Option<u64>,
	#[serde(default)]
	pub url: Option<String>,
}

pub fn load_version_profile(game_dir: &Path, version: &str) -> Result<VersionProfile> {
	let path = game_dir
		.join("versions")
		.join(version)
		.join(format!("{version}.json"));
	let content = fs::read_to_string(&path)
		.with_context(|| format!("Read version json failed: {}", path.display()))?;
	let mut profile: VersionProfile =
		serde_json::from_str(&content).context("Parse version json failed")?;

	if let Some(parent) = profile.inherits_from.take() {
		let parent_profile = load_version_profile(game_dir, &parent)?;
		profile = merge_profile(parent_profile, profile);
	}

	Ok(profile)
}

pub fn merge_profile(mut base: VersionProfile, child: VersionProfile) -> VersionProfile {
	if let Some(mc) = child.main_class {
		base.main_class = Some(mc);
	}
	if let Some(a) = child.arguments {
		if let Some(base_args) = base.arguments.as_mut() {
			base_args.jvm.extend(a.jvm);
			base_args.game.extend(a.game);
		} else {
			base.arguments = Some(a);
		}
	}
	if let Some(ma) = child.minecraft_arguments {
		base.minecraft_arguments = Some(ma);
	}
	base.libraries.extend(child.libraries);
	if let Some(assets) = child.assets {
		base.assets = Some(assets);
	}
	if child.asset_index.is_some() {
		base.asset_index = child.asset_index;
	}
	if child.downloads.is_some() {
		base.downloads = child.downloads;
	}
	base
}
