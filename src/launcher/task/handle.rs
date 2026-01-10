use crate::launcher::task::error::TaskError;
use std::sync::Arc;
use tokio::sync::{Notify, oneshot, watch};
use uuid::Uuid;

pub type TaskId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
	Pending,
	Running,
	Completed,
	Failed,
	Cancelled,
}

pub struct TaskHandle<T> {
	pub id: TaskId,
	state: Arc<tokio::sync::RwLock<TaskState>>,
	cancel_tx: Arc<watch::Sender<bool>>,
	completion: Arc<Notify>,
	result_rx: Option<oneshot::Receiver<Result<T, TaskError>>>,
}

impl<T> TaskHandle<T> {
	pub fn new(
		id: TaskId,
		state: Arc<tokio::sync::RwLock<TaskState>>,
		cancel_tx: Arc<watch::Sender<bool>>,
		completion: Arc<Notify>,
		result_rx: oneshot::Receiver<Result<T, TaskError>>,
	) -> Self {
		Self {
			id,
			state,
			cancel_tx,
			completion,
			result_rx: Some(result_rx),
		}
	}

	pub fn cancel_token(&self) -> Arc<watch::Sender<bool>> {
		Arc::clone(&self.cancel_tx)
	}

	pub fn completion_notifier(&self) -> Arc<Notify> {
		Arc::clone(&self.completion)
	}

	pub async fn state(&self) -> TaskState {
		*self.state.read().await
	}

	pub async fn cancel(&self) -> Result<(), TaskError> {
		let mut state = self.state.write().await;
		match *state {
			TaskState::Pending | TaskState::Running => {
				*state = TaskState::Cancelled;
				self.cancel_tx
					.send(true)
					.map_err(|_| TaskError::InvalidState)?;
				Ok(())
			}
			_ => Err(TaskError::InvalidState),
		}
	}

	pub async fn result(&mut self) -> Result<T, TaskError> {
		self.result_rx
			.take()
			.ok_or_else(|| TaskError::Failed("Result already consumed".into()))?
			.await
			.map_err(|_| TaskError::Failed("Channel closed".into()))?
	}
}
