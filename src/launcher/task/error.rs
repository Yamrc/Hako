use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
	#[error("Task cancelled")]
	Cancelled,

	#[error("Task failed: {0}")]
	Failed(String),

	#[error("Task timeout")]
	Timeout,

	#[error("Lock conflict: {0}")]
	LockConflict(String),

	#[error("Invalid task state")]
	InvalidState,
}

pub type TaskResult<T> = std::result::Result<T, TaskError>;
