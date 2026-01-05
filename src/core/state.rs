use crate::domain::AccountManager;
use crate::domain::game::GameInstance;
use crate::infrastructure::config::ConfigManager;
use crate::task::game::download::{DownloadProgressState, ProgressRef};
use crate::task::handle::TaskId;
use crate::task::manager::TaskManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

static APP_STATE: OnceLock<AppState> = OnceLock::new();

#[derive(Debug)]
pub struct AppState {
	pub config: Arc<ConfigManager>,
	pub accounts: AccountManager,
	pub task_manager: Arc<TaskManager>,
	pub instances: RwLock<Vec<GameInstance>>,
	pub current_instance: Mutex<Option<usize>>,
	pub task_progress: Mutex<HashMap<TaskId, ProgressRef>>,
}

impl AppState {
	pub async fn init() -> &'static Self {
		if APP_STATE.get().is_some() {
			return APP_STATE.get().expect("AppState not initialized");
		}

		let state = Self::create().await;
		state.scan_instances();
		APP_STATE.set(state).expect("Failed to initialize AppState");
		APP_STATE.get().expect("AppState not initialized")
	}

	pub fn get() -> &'static Self {
		APP_STATE.get().expect("AppState not initialized")
	}

	async fn create() -> Self {
		let config = Arc::new(ConfigManager::new().await.unwrap_or_else(|_| {
			tracing::warn!("Failed to load config, using defaults");
			ConfigManager::default()
		}));

		Self {
			config,
			accounts: AccountManager::new(),
			task_manager: Arc::new(TaskManager::new()),
			instances: RwLock::new(Vec::new()),
			current_instance: Mutex::new(None),
			task_progress: Mutex::new(HashMap::new()),
		}
	}

	pub async fn cluster_path(&self) -> PathBuf {
		let config = self.config.get().await;
		config.cluster_path.clone().unwrap_or_else(|| {
			crate::core::paths::default_minecraft_dir().unwrap_or_else(|| ".minecraft".into())
		})
	}

	pub fn scan_instances(&self) {
		let path = tokio::task::block_in_place(|| {
			tokio::runtime::Handle::current().block_on(self.cluster_path())
		});
		if let Ok(found) = crate::domain::game::InstanceScanner::scan_cluster(&path) {
			let mut guard = self.instances.write().unwrap();
			tracing::info!("Scanned {} instances from {}", found.len(), path.display());
			*guard = found;
		}
	}

	pub async fn set_cluster_path(&self, path: PathBuf) -> Result<(), anyhow::Error> {
		use crate::infrastructure::config::LauncherConfigDiff;
		let diff = LauncherConfigDiff {
			cluster_path: Some(path.clone()),
			..Default::default()
		};
		self.config.update(diff).await?;
		self.scan_instances();
		Ok(())
	}

	pub fn select_instance(&self, idx: Option<usize>) {
		*self.current_instance.lock().unwrap() = idx;
	}

	pub fn current_instance(&self) -> Option<GameInstance> {
		let idx = (*self.current_instance.lock().unwrap())?;
		self.instances.read().unwrap().get(idx).cloned()
	}

	pub fn register_progress(&self, id: TaskId) -> ProgressRef {
		let progress = Arc::new(tokio::sync::RwLock::new(DownloadProgressState::default()));
		self.task_progress
			.lock()
			.unwrap()
			.insert(id, Arc::clone(&progress));
		progress
	}
}
