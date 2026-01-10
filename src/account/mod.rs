use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
	Offline {
		username: String,
		uuid: Uuid,
	},
	Microsoft {
		username: String,
		uuid: Uuid,
		access_token: String,
	},
}

impl Account {
	pub fn offline(username: impl Into<String>) -> Self {
		let username = username.into();
		let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, username.as_bytes());
		Self::Offline { username, uuid }
	}

	pub fn username(&self) -> &str {
		match self {
			Self::Offline { username, .. } | Self::Microsoft { username, .. } => username,
		}
	}

	pub fn uuid(&self) -> &Uuid {
		match self {
			Self::Offline { uuid, .. } | Self::Microsoft { uuid, .. } => uuid,
		}
	}

	pub fn access_token(&self) -> Option<&str> {
		match self {
			Self::Microsoft { access_token, .. } => Some(access_token),
			_ => None,
		}
	}

	pub fn is_offline(&self) -> bool {
		matches!(self, Self::Offline { .. })
	}
}

pub struct AccountManager {
	accounts: RwLock<Vec<Account>>,
	current: RwLock<Option<usize>>,
}

impl AccountManager {
	pub fn new() -> Self {
		Self {
			accounts: RwLock::new(Vec::new()),
			current: RwLock::new(None),
		}
	}

	pub fn add_offline(&self, username: impl Into<String>) -> usize {
		let account = Account::offline(username);
		let mut accounts = self.accounts.write().unwrap();
		let idx = accounts.len();
		accounts.push(account);
		*self.current.write().unwrap() = Some(idx);
		idx
	}

	pub fn current(&self) -> Option<Account> {
		let idx = (*self.current.read().unwrap())?;
		self.accounts.read().unwrap().get(idx).cloned()
	}

	pub fn select(&self, idx: Option<usize>) {
		*self.current.write().unwrap() = idx;
	}

	pub fn list(&self) -> Vec<Account> {
		self.accounts.read().unwrap().clone()
	}

	pub fn remove(&self, idx: usize) {
		let mut accounts = self.accounts.write().unwrap();
		if idx < accounts.len() {
			accounts.remove(idx);
			let mut current = self.current.write().unwrap();
			if *current == Some(idx) {
				*current = None;
			} else if let Some(c) = *current {
				if c > idx {
					*current = Some(c - 1);
				}
			}
		}
	}

	// TODO: Microsoft OAuth 登录
	pub async fn login_microsoft(&self, client_id: &str) -> Result<usize, ()> {
		todo!("")
		//     1. 设备码流程获取 device_code
		//     2. 用户授权后获取 access_token
		//     3. Xbox Live 认证
		//     4. XSTS 认证
		//     5. Minecraft 认证获取 MC access_token
		//     6. 获取用户 profile (username, uuid)
		//     7. 存储到 keyring
	}
}

impl Default for AccountManager {
	fn default() -> Self {
		Self::new()
	}
}

pub fn offline_uuid(username: &str) -> Uuid {
	Uuid::new_v5(&Uuid::NAMESPACE_OID, username.as_bytes())
}
