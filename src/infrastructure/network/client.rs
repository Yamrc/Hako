use reqwest::{Client, ClientBuilder};
use serde::de::DeserializeOwned;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),

	#[error("JSON parse error: {0}")]
	Json(#[from] serde_json::Error),

	#[error("API error: {0}")]
	Api(String),

	#[error("Timeout")]
	Timeout,
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone)]
pub struct ApiClient {
	client: Client,
	base_url: Option<String>,
	timeout: Duration,
}

impl ApiClient {
	pub fn new() -> Result<Self, ApiError> {
		Self::with_config(ClientConfig::default())
	}

	pub fn with_config(config: ClientConfig) -> Result<Self, ApiError> {
		let mut builder = ClientBuilder::new()
			.use_rustls_tls()
			.tcp_nodelay(true)
			.pool_idle_timeout(Duration::from_secs(90))
			.pool_max_idle_per_host(10)
			.connect_timeout(Duration::from_secs(10));

		if let Some(timeout) = config.timeout {
			builder = builder.timeout(timeout);
		}

		if let Some(user_agent) = config.user_agent {
			builder = builder.user_agent(&user_agent);
		}

		let client = builder.build()?;

		Ok(Self {
			client,
			base_url: config.base_url,
			timeout: config.timeout.unwrap_or(Duration::from_secs(30)),
		})
	}

	pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
		self.base_url = Some(url.into());
		self
	}

	pub fn with_timeout(mut self, timeout: Duration) -> Self {
		self.timeout = timeout;
		self
	}

	fn build_url(&self, path: &str) -> String {
		match &self.base_url {
			Some(base) => {
				let base = base.trim_end_matches('/');
				let path = path.trim_start_matches('/');
				format!("{}/{}", base, path)
			}
			None => path.to_string(),
		}
	}

	pub async fn get<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.get(&url)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn get_with_query<T: DeserializeOwned, Q: serde::Serialize>(
		&self,
		path: &str,
		query: &Q,
	) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.get(&url)
			.query(query)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn post<T: DeserializeOwned, B: serde::Serialize>(
		&self,
		path: &str,
		body: &B,
	) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.post(&url)
			.json(body)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn post_raw<T: DeserializeOwned>(&self, path: &str, body: Vec<u8>) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.post(&url)
			.body(body)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn put<T: DeserializeOwned, B: serde::Serialize>(
		&self,
		path: &str,
		body: &B,
	) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.put(&url)
			.json(body)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> ApiResult<T> {
		let url = self.build_url(path);
		let response = self
			.client
			.delete(&url)
			.timeout(self.timeout)
			.send()
			.await?;

		self.handle_response(response).await
	}

	pub async fn head(&self, path: &str) -> ApiResult<reqwest::header::HeaderMap> {
		let url = self.build_url(path);
		let response = self
			.client
			.head(&url)
			.timeout(self.timeout)
			.send()
			.await?;

		if !response.status().is_success() {
			return Err(ApiError::Api(format!(
				"Request failed with status: {}",
				response.status()
			)));
		}

		Ok(response.headers().clone())
	}

	async fn handle_response<T: DeserializeOwned>(&self, response: reqwest::Response) -> ApiResult<T> {
		let status = response.status();

		if !status.is_success() {
			let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".into());
			return Err(ApiError::Api(format!(
				"Request failed with status {}: {}",
				status, error_text
			)));
		}

		let text = response.text().await?;
		serde_json::from_str(&text).map_err(ApiError::Json)
	}

	pub fn raw_client(&self) -> &Client {
		&self.client
	}
}

impl Default for ApiClient {
	fn default() -> Self {
		Self::new().expect("Failed to create ApiClient")
	}
}

#[derive(Clone, Default)]
pub struct ClientConfig {
	pub base_url: Option<String>,
	pub timeout: Option<Duration>,
	pub user_agent: Option<String>,
}

impl ClientConfig {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
		self.base_url = Some(url.into());
		self
	}

	pub fn with_timeout(mut self, timeout: Duration) -> Self {
		self.timeout = Some(timeout);
		self
	}

	pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
		self.user_agent = Some(agent.into());
		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Serialize, Deserialize)]
	struct TestResponse {
		message: String,
	}

	#[tokio::test]
	async fn test_api_client_get() {
		let client = ApiClient::new().unwrap();
		let result: ApiResult<TestResponse> = client.get("https://httpbin.org/get").await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_api_client_with_base_url() {
		let client = ApiClient::new()
			.unwrap()
			.with_base_url("https://httpbin.org");

		let result: ApiResult<TestResponse> = client.get("/get").await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_api_client_post() {
		let client = ApiClient::new().unwrap();
		let body = serde_json::json!({"key": "value"});

		let result: ApiResult<TestResponse> = client.post("https://httpbin.org/post", &body).await;
		assert!(result.is_ok());
	}
}
