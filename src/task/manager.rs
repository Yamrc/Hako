use crate::task::error::{TaskError, TaskResult};
use crate::task::executor::TaskExecutor;
use crate::task::handle::{TaskHandle, TaskId};
use crate::task::lock::LockManager;
use crate::task::task_trait::Task;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock, watch};

#[derive(Debug)]
pub struct TaskManager {
	executor: TaskExecutor,
	tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
}

#[derive(Debug)]
struct TaskInfo {
	cancel_tx: Arc<watch::Sender<bool>>,
	completion: Arc<Notify>,
}

impl TaskManager {
	pub fn new() -> Self {
		let lock_manager = Arc::new(LockManager::new());
		Self {
			executor: TaskExecutor::new(lock_manager, Some(5)),
			tasks: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	pub async fn submit<T: Task>(&self, task: T) -> TaskResult<TaskHandle<T::Output>> {
		let handle = self.executor.submit(task).await?;
		self.track_task(&handle).await;
		Ok(handle)
	}

	pub async fn cancel(&self, task_id: TaskId) -> TaskResult<()> {
		let tasks = self.tasks.read().await;
		let info = tasks.get(&task_id).ok_or(TaskError::InvalidState)?;
		info.cancel_tx
			.send(true)
			.map_err(|_| TaskError::InvalidState)
	}

	async fn track_task<T>(&self, handle: &TaskHandle<T>) {
		let task_id = handle.id;
		let info = TaskInfo {
			cancel_tx: handle.cancel_token(),
			completion: handle.completion_notifier(),
		};

		self.tasks.write().await.insert(task_id, info);

		let tasks = Arc::clone(&self.tasks);
		let completion = handle.completion_notifier();

		tokio::spawn(async move {
			completion.notified().await;
			tasks.write().await.remove(&task_id);
		});
	}
}

impl Default for TaskManager {
	fn default() -> Self {
		Self::new()
	}
}
