use crate::task::error::{TaskError, TaskResult};
use crate::task::executor::{BlockingExecutor, ConcurrentExecutor};
use crate::task::handle::{TaskHandle, TaskId};
use crate::task::lock::LockManager;
use crate::task::main_task::{BlockingTask, ConcurrentTask};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock, watch};

pub struct TaskManager {
	blocking_executor: BlockingExecutor,
	concurrent_executor: ConcurrentExecutor,
	tasks: RwLock<HashMap<TaskId, TaskInfo>>,
}

struct TaskInfo {
	cancel_tx: Arc<watch::Sender<bool>>,
	completion: Arc<Notify>,
}

impl TaskManager {
	pub fn new() -> Self {
		let lock_manager = Arc::new(LockManager::new());
		Self {
			blocking_executor: BlockingExecutor::new(Arc::clone(&lock_manager)),
			concurrent_executor: ConcurrentExecutor::new(lock_manager, Some(5)),
			tasks: RwLock::new(HashMap::new()),
		}
	}

	pub async fn submit_blocking<T: BlockingTask>(
		&self,
		task: T,
	) -> TaskResult<TaskHandle<T::Output>> {
		let locks = task.locks();
		let handle = self.blocking_executor.submit(task, locks).await?;
		self.track_task(&handle).await;
		Ok(handle)
	}

	pub async fn submit_concurrent<T: ConcurrentTask>(
		&self,
		task: T,
	) -> TaskResult<TaskHandle<T::Output>> {
		let locks = task.locks();
		let handle = self.concurrent_executor.submit(task, locks).await?;
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

		let tasks = &self.tasks as *const _ as usize;
		let completion = handle.completion_notifier();

		tokio::spawn(async move {
			completion.notified().await;
			let tasks = unsafe { &*(tasks as *const RwLock<HashMap<TaskId, TaskInfo>>) };
			tasks.write().await.remove(&task_id);
		});
	}
}

impl Default for TaskManager {
	fn default() -> Self {
		Self::new()
	}
}
