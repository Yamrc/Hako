use crate::game::args::{Features, current_arch, current_os_key, rule_allows};
use crate::game::profile::{Library, VersionProfile, load_version_profile};
use crate::net::download::{DownloadClient, DownloadRequest};
use crate::task::error::{TaskError, TaskResult};
use crate::task::lock::LockKey;
use crate::task::main_task::{ConcurrentTask, TaskContext, TaskType};
use crate::task::sub_task::{SubTask, SubTaskChain, SubTaskContext};
use anyhow::Context;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{OnceCell, RwLock, watch};

#[derive(Clone, Debug, Default)]
pub struct DownloadProgressState {
	pub message: String,
	pub downloaded: u64,
	pub total: Option<u64>,
	pub speed_bps: f64,
	pub finished: bool,
}

pub type ProgressRef = Arc<RwLock<DownloadProgressState>>;

struct DownloadContext {
	client: DownloadClient,
	game_dir: PathBuf,
	version_id: String,
	progress: Option<ProgressRef>,
	profile: OnceCell<VersionProfile>,
}

impl DownloadContext {
	fn new(
		game_dir: PathBuf,
		version_id: String,
		progress: Option<ProgressRef>,
	) -> TaskResult<Self> {
		Ok(Self {
			client: DownloadClient::new().map_err(|e| TaskError::Failed(e.to_string()))?,
			game_dir,
			version_id,
			progress,
			profile: OnceCell::new(),
		})
	}

	async fn set_progress(
		&self,
		message: &str,
		downloaded: u64,
		total: Option<u64>,
		speed: f64,
		finished: bool,
	) {
		if let Some(p) = &self.progress {
			let mut guard = p.write().await;
			guard.message = message.to_string();
			guard.downloaded = downloaded;
			guard.total = total;
			guard.speed_bps = speed;
			guard.finished = finished;
		}
	}

	fn profile(&self) -> Option<&VersionProfile> {
		self.profile.get()
	}
}

pub struct DownloadGameTask {
	pub cluster_path: PathBuf,
	pub version: String,
	pub progress: Option<ProgressRef>,
}

impl TaskType for DownloadGameTask {
	const TYPE_NAME: &'static str = "download_game";
}

#[async_trait::async_trait]
impl ConcurrentTask for DownloadGameTask {
	type Output = ();

	fn locks(&self) -> Vec<LockKey> {
		vec![LockKey::resource("download_game", &self.version)]
	}

	fn max_concurrent(&self) -> Option<usize> {
		Some(2)
	}

	async fn execute(&mut self, ctx: &TaskContext) -> TaskResult<Self::Output> {
		let shared = Arc::new(DownloadContext::new(
			self.cluster_path.clone(),
			self.version.clone(),
			self.progress.clone(),
		)?);

		let mut chain = SubTaskChain::new();
		chain.add(EnsureProfileTask(Arc::clone(&shared)));
		chain.add_parallel(
			vec![
				Arc::new(ClientJarTask(Arc::clone(&shared))) as Arc<dyn SubTask>,
				Arc::new(CoreLibTask(Arc::clone(&shared))) as Arc<dyn SubTask>,
				Arc::new(AssetsTask(Arc::clone(&shared))) as Arc<dyn SubTask>,
			],
			None,
		);

		let sub_ctx = SubTaskContext::new(ctx.cancelled_receiver());
		chain.execute(&sub_ctx).await?;

		shared
			.set_progress(
				&format!("{} 下载完成", shared.version_id),
				0,
				None,
				0.0,
				true,
			)
			.await;
		Ok(())
	}
}

struct EnsureProfileTask(Arc<DownloadContext>);

#[async_trait::async_trait]
impl SubTask for EnsureProfileTask {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		let s = &self.0;
		if s.profile.get().is_some() {
			return Ok(());
		}

		let version_json = s
			.game_dir
			.join("versions")
			.join(&s.version_id)
			.join(format!("{}.json", s.version_id));

		if !version_json.exists() {
			s.set_progress(
				&format!("下载版本元数据 {}", s.version_id),
				0,
				None,
				0.0,
				false,
			)
			.await;

			if let Some(dir) = version_json.parent() {
				fs::create_dir_all(dir)
					.await
					.map_err(|e| TaskError::Failed(e.to_string()))?;
			}

			let meta_url = resolve_version_url(&s.version_id).await?;
			s.client
				.download(
					DownloadRequest::new(meta_url, &version_json),
					|_| {},
					Some(ctx.cancelled.clone()),
				)
				.await
				.map_err(|e| TaskError::Failed(e.to_string()))?;
		}

		let profile = load_version_profile(&s.game_dir, &s.version_id)
			.map_err(|e| TaskError::Failed(e.to_string()))?;
		let _ = s.profile.set(profile);
		Ok(())
	}
}

struct ClientJarTask(Arc<DownloadContext>);

#[async_trait::async_trait]
impl SubTask for ClientJarTask {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		let s = &self.0;
		let profile = s
			.profile()
			.ok_or_else(|| TaskError::Failed("profile missing".into()))?;

		let Some(client_dl) = profile.downloads.as_ref().and_then(|d| d.client.as_ref()) else {
			return Ok(());
		};
		let Some(url) = &client_dl.url else {
			return Ok(());
		};

		let dest = s
			.game_dir
			.join("versions")
			.join(&s.version_id)
			.join(format!("{}.jar", s.version_id));

		if dest.exists() {
			return Ok(());
		}

		check_cancel(&ctx.cancelled)?;
		// let version_id = s.version_id.clone();
		s.client
			.download(
				DownloadRequest::new(url.clone(), dest),
				|_| { /* 进度回调 */ },
				Some(ctx.cancelled.clone()),
			)
			.await
			.map_err(|e| TaskError::Failed(e.to_string()))
	}
}

struct CoreLibTask(Arc<DownloadContext>);

#[async_trait::async_trait]
impl SubTask for CoreLibTask {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		let s = &self.0;
		let profile = s
			.profile()
			.ok_or_else(|| TaskError::Failed("profile missing".into()))?;

		let features = Features::default();
		let os_key = current_os_key();
		let arch = current_arch();

		let requests: Vec<_> = profile
			.libraries
			.iter()
			.filter(|lib| rule_allows(lib.rules.as_ref(), os_key, arch, &features))
			.filter_map(|lib| library_request(&s.game_dir, lib, os_key))
			.filter(|req| !req.dest.exists())
			.collect();

		for req in requests {
			check_cancel(&ctx.cancelled)?;
			s.client
				.download(req, |_| {}, Some(ctx.cancelled.clone()))
				.await
				.map_err(|e| TaskError::Failed(e.to_string()))?;
		}
		Ok(())
	}
}

struct AssetsTask(Arc<DownloadContext>);

#[async_trait::async_trait]
impl SubTask for AssetsTask {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		let s = &self.0;
		let profile = s
			.profile()
			.ok_or_else(|| TaskError::Failed("profile missing".into()))?;

		let assets_id = profile.assets.as_deref().unwrap_or(&s.version_id);
		let assets_dir = s.game_dir.join("assets");
		let index_path = assets_dir.join("indexes").join(format!("{assets_id}.json"));

		if !index_path.exists() {
			if let Some(url) = profile.asset_index.as_ref().and_then(|i| i.url.as_ref()) {
				check_cancel(&ctx.cancelled)?;
				s.client
					.download(
						DownloadRequest::new(url.clone(), index_path.clone()),
						|_| {},
						Some(ctx.cancelled.clone()),
					)
					.await
					.map_err(|e| TaskError::Failed(e.to_string()))?;
			} else {
				return Ok(());
			}
		}

		if !index_path.exists() {
			return Ok(());
		}

		let index: AssetIndex = {
			let content = fs::read_to_string(&index_path)
				.await
				.context("read asset index")
				.map_err(|e| TaskError::Failed(e.to_string()))?;
			serde_json::from_str(&content)
				.context("parse asset index")
				.map_err(|e| TaskError::Failed(e.to_string()))?
		};

		let requests: Vec<_> = index
			.objects
			.values()
			.filter(|a| a.hash.len() >= 2)
			.filter_map(|a| {
				let subdir = &a.hash[..2];
				let dest = assets_dir.join("objects").join(subdir).join(&a.hash);
				if dest.exists() {
					return None;
				}
				let url = format!(
					"https://resources.download.minecraft.net/{}/{}",
					subdir, a.hash
				);
				Some(DownloadRequest::new(url, dest))
			})
			.collect();

		for req in requests {
			check_cancel(&ctx.cancelled)?;
			s.client
				.download(req, |_| {}, Some(ctx.cancelled.clone()))
				.await
				.map_err(|e| TaskError::Failed(e.to_string()))?;
		}
		Ok(())
	}
}

fn check_cancel(cancel: &watch::Receiver<bool>) -> TaskResult<()> {
	if *cancel.borrow() {
		Err(TaskError::Cancelled)
	} else {
		Ok(())
	}
}

fn library_request(game_dir: &Path, lib: &Library, os_key: &str) -> Option<DownloadRequest> {
	let downloads = lib.downloads.as_ref()?;

	if let Some(natives) = &lib.natives {
		if natives.contains_key(os_key) {
			if let Some(classifiers) = &downloads.classifiers {
				let key = format!("natives-{os_key}");
				if let Some(artifact) = classifiers.get(&key) {
					if let (Some(path), Some(url)) = (&artifact.path, &artifact.url) {
						let dest = game_dir
							.join("libraries")
							.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
						return Some(DownloadRequest::new(url.clone(), dest));
					}
				}
			}
		}
	}

	if let Some(artifact) = &downloads.artifact {
		if let (Some(path), Some(url)) = (&artifact.path, &artifact.url) {
			let dest = game_dir
				.join("libraries")
				.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
			return Some(DownloadRequest::new(url.clone(), dest));
		}
	}

	None
}

async fn resolve_version_url(version_id: &str) -> TaskResult<String> {
	const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest.json";

	let resp = reqwest::get(MANIFEST)
		.await
		.map_err(|e| TaskError::Failed(format!("Fetch manifest: {e}")))?;
	let text = resp
		.text()
		.await
		.map_err(|e| TaskError::Failed(format!("Read manifest: {e}")))?;
	let manifest: VersionManifest = serde_json::from_str(&text)
		.map_err(|e| TaskError::Failed(format!("Parse manifest: {e}")))?;

	manifest
		.versions
		.into_iter()
		.find(|v| v.id == version_id)
		.map(|v| v.url)
		.ok_or_else(|| TaskError::Failed(format!("Version {} not found", version_id)))
}

#[derive(Deserialize)]
struct AssetIndex {
	objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
	hash: String,
}

#[derive(Deserialize)]
struct VersionManifest {
	versions: Vec<VersionRef>,
}

#[derive(Deserialize)]
struct VersionRef {
	id: String,
	url: String,
}
