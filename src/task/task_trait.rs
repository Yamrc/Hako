use crate::task::error::TaskResult;
use crate::task::lock::LockKey;
use async_trait::async_trait;

pub trait TaskType: Send + Sync + 'static {
	const TYPE_NAME: &'static str;

	fn description(&self) -> String {
		self.type_name().to_string()
	}

	fn type_name(&self) -> &'static str {
		Self::TYPE_NAME
	}
}

#[async_trait]
pub trait Task: TaskType + Send + Sync {
	type Output: Send + 'static;

	async fn execute(&mut self, ctx: &TaskContext) -> TaskResult<Self::Output>;

	fn locks(&self) -> Vec<LockKey> {
		vec![]
	}

	fn queueable(&self) -> bool {
		true
	}

	fn max_concurrent(&self) -> Option<usize> {
		None
	}

	fn requires_global_lock(&self) -> bool {
		false
	}
}

pub struct TaskContext {
	cancelled: tokio::sync::watch::Receiver<bool>,
}

impl TaskContext {
	pub fn new(cancelled: tokio::sync::watch::Receiver<bool>) -> Self {
		Self { cancelled }
	}

	pub fn is_cancelled(&self) -> bool {
		*self.cancelled.borrow()
	}

	pub async fn cancelled(&mut self) {
		let _ = self.cancelled.changed().await;
	}

	pub fn cancelled_receiver(&self) -> tokio::sync::watch::Receiver<bool> {
		self.cancelled.clone()
	}
}
