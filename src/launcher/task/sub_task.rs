use crate::launcher::task::error::TaskError;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
	pub max_retries: u32,
	pub retry_delay_ms: u64,
}

impl Default for RetryPolicy {
	fn default() -> Self {
		Self {
			max_retries: 0,
			retry_delay_ms: 1000,
		}
	}
}

#[async_trait]
pub trait SubTask: Send + Sync {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError>;

	fn condition(&self, _ctx: &SubTaskContext) -> bool {
		true
	}

	fn retry_policy(&self) -> RetryPolicy {
		RetryPolicy::default()
	}
}

#[derive(Clone)]
pub struct SubTaskContext {
	pub cancelled: tokio::sync::watch::Receiver<bool>,
}

impl SubTaskContext {
	pub fn new(cancelled: tokio::sync::watch::Receiver<bool>) -> Self {
		Self { cancelled }
	}

	pub fn is_cancelled(&self) -> bool {
		*self.cancelled.borrow()
	}
}

enum ChainItem {
	Single(Arc<dyn SubTask>),
	Parallel {
		tasks: Vec<Arc<dyn SubTask>>,
		limit: Option<usize>,
	},
}

pub struct SubTaskChain {
	items: Vec<ChainItem>,
}

impl SubTaskChain {
	pub fn new() -> Self {
		Self { items: Vec::new() }
	}

	pub fn add<T: SubTask + 'static>(&mut self, task: T) {
		self.items.push(ChainItem::Single(Arc::new(task)));
	}

	pub fn add_parallel<I>(&mut self, tasks: I, limit: Option<usize>)
	where
		I: IntoIterator<Item = Arc<dyn SubTask>>,
	{
		let boxed: Vec<Arc<dyn SubTask>> = tasks.into_iter().collect();
		self.items.push(ChainItem::Parallel {
			tasks: boxed,
			limit,
		});
	}

	pub async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		for item in &self.items {
			match item {
				ChainItem::Single(task) => run_with_retry(task.clone(), ctx).await?,
				ChainItem::Parallel { tasks, limit } => {
					execute_parallel(tasks, *limit, ctx).await?;
				}
			}
		}
		Ok(())
	}
}

impl Default for SubTaskChain {
	fn default() -> Self {
		Self::new()
	}
}

async fn run_with_retry(task: Arc<dyn SubTask>, ctx: &SubTaskContext) -> Result<(), TaskError> {
	if !task.condition(ctx) {
		return Ok(());
	}
	let policy = task.retry_policy();
	let mut last_error = None;
	for attempt in 0..=policy.max_retries {
		if ctx.is_cancelled() {
			return Err(TaskError::Cancelled);
		}
		match task.execute(ctx).await {
			Ok(()) => return Ok(()),
			Err(e) => {
				last_error = Some(e);
				if attempt < policy.max_retries {
					tokio::time::sleep(Duration::from_millis(policy.retry_delay_ms)).await;
				}
			}
		}
	}
	Err(last_error.unwrap_or(TaskError::Failed("subtask failed".into())))
}

async fn execute_parallel(
	tasks: &[Arc<dyn SubTask>],
	limit: Option<usize>,
	ctx: &SubTaskContext,
) -> Result<(), TaskError> {
	if tasks.is_empty() {
		return Ok(());
	}
	let limit = limit.unwrap_or(tasks.len()).max(1);
	let mut set = JoinSet::new();
	let mut idx = 0;
	let total = tasks.len();
	let mut running = 0usize;
	let mut last_error = None;

	while idx < total || running > 0 {
		while running < limit && idx < total {
			let task = tasks[idx].clone();
			let ctx_clone = ctx.clone();
			set.spawn(async move { run_with_retry(task, &ctx_clone).await });
			idx += 1;
			running += 1;
		}

		if ctx.is_cancelled() {
			return Err(TaskError::Cancelled);
		}

		if let Some(res) = set.join_next().await {
			running -= 1;
			match res {
				Ok(Ok(())) => {}
				Ok(Err(e)) => {
					last_error = Some(e);
					break;
				}
				Err(join_err) => {
					last_error = Some(TaskError::Failed(format!("subtask panicked: {join_err}")));
					break;
				}
			}
		}
	}

	if let Some(e) = last_error {
		return Err(e);
	}

	Ok(())
}
