use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockKey {
	pub resource_type: &'static str,
	pub resource_id: String,
}

impl LockKey {
	pub fn global(resource_type: &'static str) -> Self {
		Self {
			resource_type,
			resource_id: "global".into(),
		}
	}

	pub fn resource(resource_type: &'static str, resource_id: impl Into<String>) -> Self {
		Self {
			resource_type,
			resource_id: resource_id.into(),
		}
	}
}

#[derive(Debug)]
pub struct LockManager {
	locks: Arc<RwLock<HashSet<LockKey>>>,
}

impl LockManager {
	pub fn new() -> Self {
		Self {
			locks: Arc::new(RwLock::new(HashSet::new())),
		}
	}

	pub async fn try_acquire(&self, keys: &[LockKey]) -> Result<(), String> {
		if keys.is_empty() {
			return Ok(());
		}

		let mut locks = self.locks.write().await;

		for key in keys {
			if locks.contains(key) {
				return Err(format!("Lock conflict: {:?}", key));
			}
		}

		for key in keys {
			locks.insert(key.clone());
		}

		Ok(())
	}

	pub async fn release(&self, keys: &[LockKey]) {
		if keys.is_empty() {
			return;
		}
		let mut locks = self.locks.write().await;
		for key in keys {
			locks.remove(key);
		}
	}
}

impl Default for LockManager {
	fn default() -> Self {
		Self::new()
	}
}
