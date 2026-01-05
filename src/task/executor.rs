use crate::task::error::{TaskError, TaskResult};
use crate::task::handle::{TaskHandle, TaskId, TaskState};
use crate::task::lock::LockManager;
use crate::task::task_trait::{Task, TaskContext};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock, Semaphore, oneshot, watch};
use uuid::Uuid;

#[derive(Debug)]
struct PendingTask {
	task_id: TaskId,
	notify: Arc<Notify>,
}

#[derive(Debug)]
pub struct TaskExecutor {
	lock_manager: Arc<LockManager>,
	running: Arc<RwLock<HashMap<&'static str, TaskId>>>,
	queues: Arc<RwLock<HashMap<&'static str, Vec<PendingTask>>>>,
	global_semaphore: Option<Arc<Semaphore>>,
	type_semaphores: RwLock<HashMap<&'static str, Arc<Semaphore>>>,
}

impl TaskExecutor {
	pub fn new(lock_manager: Arc<LockManager>, max_concurrent: Option<usize>) -> Self {
		Self {
			lock_manager,
			running: Arc::new(RwLock::new(HashMap::new())),
			queues: Arc::new(RwLock::new(HashMap::new())),
			global_semaphore: max_concurrent.map(|n| Arc::new(Semaphore::new(n))),
			type_semaphores: RwLock::new(HashMap::new()),
		}
	}

	pub async fn submit<T: Task>(&self, mut task: T) -> TaskResult<TaskHandle<T::Output>> {
		let task_type = task.type_name();
		let task_id = Uuid::new_v4();
		let requires_global_lock = task.requires_global_lock();

		if requires_global_lock {
			let running = self.running.read().await;
			if running.contains_key(task_type) {
				if !task.queueable() {
					return Err(TaskError::LockConflict(format!(
						"{} already running",
						task_type
					)));
				}
				drop(running);

				let notify = Arc::new(Notify::new());
				{
					let mut queues = self.queues.write().await;
					queues.entry(task_type).or_default().push(PendingTask {
						task_id,
						notify: Arc::clone(&notify),
					});
				}
				notify.notified().await;
			}
		}

		self.lock_manager
			.try_acquire(&task.locks())
			.await
			.map_err(TaskError::LockConflict)?;

		let (cancel_tx, cancel_rx) = watch::channel(false);
		let (result_tx, result_rx) = oneshot::channel();
		let state = Arc::new(RwLock::new(TaskState::Pending));
		let completion = Arc::new(Notify::new());
		let cancel_tx = Arc::new(cancel_tx);

		let handle = TaskHandle::new(
			task_id,
			Arc::clone(&state),
			Arc::clone(&cancel_tx),
			Arc::clone(&completion),
			result_rx,
		);

		if requires_global_lock {
			self.running.write().await.insert(task_type, task_id);
		}

		let lock_manager = Arc::clone(&self.lock_manager);
		let running = Arc::clone(&self.running);
		let queues = Arc::clone(&self.queues);

		let type_sem = if let Some(limit) = task.max_concurrent() {
			let mut sems = self.type_semaphores.write().await;
			Some(Arc::clone(
				sems.entry(task_type)
					.or_insert_with(|| Arc::new(Semaphore::new(limit))),
			))
		} else {
			None
		};

		let global_sem = self.global_semaphore.clone();

		tokio::spawn(async move {
			let _global_permit = if let Some(sem) = &global_sem {
				Some(sem.acquire().await)
			} else {
				None
			};
			let _type_permit = if let Some(sem) = &type_sem {
				Some(sem.acquire().await)
			} else {
				None
			};

			*state.write().await = TaskState::Running;

			let ctx = TaskContext::new(cancel_rx);
			let result = task.execute(&ctx).await;

			lock_manager.release(&task.locks()).await;

			if requires_global_lock {
				running.write().await.remove(task_type);
				let mut q = queues.write().await;
				if let Some(queue) = q.get_mut(task_type) {
					if let Some(pending) = queue.pop() {
						pending.notify.notify_one();
					} else {
						q.remove(task_type);
					}
				}
			}

			*state.write().await = if result.is_ok() {
				TaskState::Completed
			} else {
				TaskState::Failed
			};
			let _ = result_tx.send(result);
			completion.notify_waiters();
		});

		Ok(handle)
	}
}
