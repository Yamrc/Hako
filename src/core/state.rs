use crate::account::AccountManager;
use crate::config::manager::ConfigManager;
use crate::game::instance::GameInstance;
use crate::task::game::download::{DownloadProgressState, ProgressRef};
use crate::task::handle::TaskId;
use crate::task::manager::TaskManager;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

static APP_STATE: OnceLock<AppState> = OnceLock::new();

pub struct AppState {
	pub config: ConfigManager,
	pub accounts: AccountManager,
	pub task_manager: Arc<TaskManager>,
	pub instances: RwLock<Vec<GameInstance>>,
	pub current_instance: Mutex<Option<usize>>,
	pub task_progress: Mutex<HashMap<TaskId, ProgressRef>>,
}

impl AppState {
	pub fn init() -> &'static Self {
		APP_STATE.get_or_init(|| {
			let state = Self::create();
			state.scan_instances();
			state
		})
	}

	pub fn get() -> &'static Self {
		APP_STATE.get().expect("AppState not initialized")
	}

	fn create() -> Self {
		Self {
			config: ConfigManager::default(),
			accounts: AccountManager::new(),
			task_manager: Arc::new(TaskManager::new()),
			instances: RwLock::new(Vec::new()),
			current_instance: Mutex::new(None),
			task_progress: Mutex::new(HashMap::new()),
		}
	}

	pub fn cluster_path(&self) -> PathBuf {
		self.config.get().cluster_path.unwrap_or_else(|| {
			crate::core::paths::default_minecraft_dir().unwrap_or_else(|| ".minecraft".into())
		})
	}

	pub fn scan_instances(&self) {
		let path = self.cluster_path();
		if let Ok(found) = crate::game::instance::InstanceScanner::scan_cluster(&path) {
			let mut guard = self.instances.write().unwrap();
			tracing::info!("Scanned {} instances from {}", found.len(), path.display());
			*guard = found;
		}
	}

	pub fn set_cluster_path(&self, path: PathBuf) {
		let _ = self.config.update(|c| c.cluster_path = Some(path));
		self.scan_instances();
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
