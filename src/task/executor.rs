use crate::task::error::{TaskError, TaskResult};
use crate::task::handle::{TaskHandle, TaskId, TaskState};
use crate::task::lock::{LockKey, LockManager};
use crate::task::main_task::{BlockingTask, ConcurrentTask, TaskContext};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock, Semaphore, oneshot, watch};
use uuid::Uuid;

struct PendingTask {
	task_id: TaskId,
	notify: Arc<Notify>,
}

pub struct BlockingExecutor {
	lock_manager: Arc<LockManager>,
	running: RwLock<HashMap<&'static str, TaskId>>,
	queues: RwLock<HashMap<&'static str, Vec<PendingTask>>>,
}

impl BlockingExecutor {
	pub fn new(lock_manager: Arc<LockManager>) -> Self {
		Self {
			lock_manager,
			running: RwLock::new(HashMap::new()),
			queues: RwLock::new(HashMap::new()),
		}
	}

	pub async fn submit<T: BlockingTask>(
		&self,
		mut task: T,
		locks: Vec<LockKey>,
	) -> TaskResult<TaskHandle<T::Output>> {
		let task_type = T::TYPE_NAME;
		let task_id = Uuid::new_v4();
		let has_global_lock = locks.iter().any(|l| l.resource_id == "global");

		if has_global_lock {
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
			.try_acquire(&locks)
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

		if has_global_lock {
			self.running.write().await.insert(task_type, task_id);
		}

		let lock_manager = Arc::clone(&self.lock_manager);
		let running = &self.running as *const _ as usize;
		let queues = &self.queues as *const _ as usize;

		tokio::spawn(async move {
			*state.write().await = TaskState::Running;

			let ctx = TaskContext::new(cancel_rx);
			let result = task.execute(&ctx).await;

			let running = unsafe { &*(running as *const RwLock<HashMap<&'static str, TaskId>>) };
			let queues =
				unsafe { &*(queues as *const RwLock<HashMap<&'static str, Vec<PendingTask>>>) };

			lock_manager.release(&locks).await;

			if has_global_lock {
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

pub struct ConcurrentExecutor {
	lock_manager: Arc<LockManager>,
	global_semaphore: Option<Arc<Semaphore>>,
	type_semaphores: RwLock<HashMap<&'static str, Arc<Semaphore>>>,
}

impl ConcurrentExecutor {
	pub fn new(lock_manager: Arc<LockManager>, max_concurrent: Option<usize>) -> Self {
		Self {
			lock_manager,
			global_semaphore: max_concurrent.map(|n| Arc::new(Semaphore::new(n))),
			type_semaphores: RwLock::new(HashMap::new()),
		}
	}

	pub async fn submit<T: ConcurrentTask>(
		&self,
		mut task: T,
		locks: Vec<LockKey>,
	) -> TaskResult<TaskHandle<T::Output>> {
		let task_id = Uuid::new_v4();

		self.lock_manager
			.try_acquire(&locks)
			.await
			.map_err(TaskError::LockConflict)?;

		let (cancel_tx, cancel_rx) = watch::channel(false);
		let (result_tx, result_rx) = oneshot::channel();
		let state = Arc::new(RwLock::new(TaskState::Pending));
		let completion = Arc::new(Notify::new());

		let handle = TaskHandle::new(
			task_id,
			Arc::clone(&state),
			Arc::new(cancel_tx),
			Arc::clone(&completion),
			result_rx,
		);

		let type_sem = if let Some(limit) = task.max_concurrent() {
			let mut sems = self.type_semaphores.write().await;
			Some(Arc::clone(
				sems.entry(T::TYPE_NAME)
					.or_insert_with(|| Arc::new(Semaphore::new(limit))),
			))
		} else {
			None
		};

		let global_sem = self.global_semaphore.clone();
		let lock_manager = Arc::clone(&self.lock_manager);

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

			lock_manager.release(&locks).await;

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
