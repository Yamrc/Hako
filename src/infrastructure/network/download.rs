use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::TryStreamExt;
use reqwest::header::RANGE;
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::{debug, warn};

const DEFAULT_RETRY: usize = 3;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
pub enum Checksum {
	Sha256(String),
}

#[derive(Clone, Debug)]
pub struct DownloadRequest {
	pub url: String,
	pub dest: PathBuf,
	pub checksum: Option<Checksum>,
	pub retry: usize,
	pub timeout: Duration,
}

impl DownloadRequest {
	pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
		Self {
			url: url.into(),
			dest: dest.into(),
			checksum: None,
			retry: DEFAULT_RETRY,
			timeout: DEFAULT_TIMEOUT,
		}
	}

	pub fn with_checksum(mut self, checksum: Checksum) -> Self {
		self.checksum = Some(checksum);
		self
	}
}

#[derive(Clone, Debug)]
pub struct DownloadProgress {
	pub downloaded: u64,
	pub total: Option<u64>,
	pub speed_bps: f64,
}

#[derive(Error, Debug)]
pub enum DownloadError {
	#[error("http error: {0}")]
	Http(#[from] reqwest::Error),
	#[error("io error: {0}")]
	Io(#[from] std::io::Error),
	#[error("unexpected status code: {0}")]
	UnexpectedStatus(StatusCode),
	#[error("checksum mismatch")]
	ChecksumMismatch,
	#[error("download cancelled")]
	Cancelled,
	#[error("retry exhausted after {0} attempts")]
	RetryExhausted(usize),
}

pub struct DownloadClient {
	client: Client,
}

impl DownloadClient {
	pub fn new() -> Result<Self, DownloadError> {
		let client = Client::builder()
			.read_timeout(Duration::from_secs(600)) // 我不信还能timeout
			.build()?;

		Ok(Self { client })
	}

	pub async fn download<F>(
		&self,
		request: DownloadRequest,
		mut on_progress: F,
		cancel: Option<watch::Receiver<bool>>,
	) -> Result<(), DownloadError>
	where
		F: FnMut(DownloadProgress),
	{
		if let Some(parent) = request.dest.parent() {
			fs::create_dir_all(parent).await?;
		}

		let temp_path = request.dest.with_extension("hako.part");

		debug!(
			"download start url={} dest={} resume_from={}",
			request.url,
			request.dest.display(),
			fs::metadata(&temp_path).await.map(|m| m.len()).unwrap_or(0)
		);

		if let Some(checksum) = &request.checksum {
			if file_matches_checksum(&request.dest, checksum)
				.await
				.unwrap_or(false)
			{
				let size = fs::metadata(&request.dest)
					.await
					.map(|m| m.len())
					.unwrap_or(0);
				on_progress(DownloadProgress {
					downloaded: size,
					total: Some(size),
					speed_bps: 0.0,
				});
				return Ok(());
			}
		}

		let mut start_from = fs::metadata(&temp_path).await.map(|m| m.len()).unwrap_or(0);

		let downloaded = Arc::new(AtomicU64::new(start_from));
		let mut last_instant = Instant::now();
		let mut last_downloaded = start_from;

		let download_result = self
			.download_single(
				&request,
				&mut start_from,
				&temp_path,
				&downloaded,
				&mut last_instant,
				&mut last_downloaded,
				&mut on_progress,
				cancel.clone(),
			)
			.await;

		let final_downloaded = downloaded.load(Ordering::Relaxed);
		on_progress(DownloadProgress {
			downloaded: final_downloaded,
			total: None,
			speed_bps: 0.0,
		});

		match download_result {
			Ok(_) => {
				if let Some(checksum) = &request.checksum {
					if !file_matches_checksum(&temp_path, checksum)
						.await
						.unwrap_or(false)
					{
						return Err(DownloadError::ChecksumMismatch);
					}
				}

				fs::rename(&temp_path, &request.dest).await?;
				Ok(())
			}
			Err(e) => Err(e),
		}
	}

	async fn download_single<F>(
		&self,
		request: &DownloadRequest,
		start_from: &mut u64,
		temp_path: &Path,
		downloaded: &Arc<AtomicU64>,
		last_instant: &mut Instant,
		last_downloaded: &mut u64,
		on_progress: &mut F,
		cancel: Option<watch::Receiver<bool>>,
	) -> Result<(), DownloadError>
	where
		F: FnMut(DownloadProgress),
	{
		let mut attempt = 0;

		loop {
			if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
				return Err(DownloadError::Cancelled);
			}

			let mut file = OpenOptions::new()
				.create(true)
				.write(true)
				.read(true)
				.open(temp_path)
				.await?;

			if *start_from > 0 {
				file.seek(std::io::SeekFrom::Start(*start_from)).await?;
			} else {
				file.set_len(0).await?;
			}

			let mut req = self.client.get(&request.url).timeout(request.timeout);
			if *start_from > 0 {
				req = req.header(RANGE, format!("bytes={}-", start_from));
			}

			let resp = match req.send().await {
				Ok(r) => r,
				Err(e) => {
					if attempt >= request.retry {
						return Err(DownloadError::Http(e));
					}
					attempt += 1;
					warn!(
						"download attempt {} failed (net): {}, will retry",
						attempt, e
					);
					tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
					continue;
				}
			};

			let status = resp.status();
			let is_partial = status == StatusCode::PARTIAL_CONTENT;

			if *start_from > 0 && !is_partial {
				if attempt >= request.retry {
					return Err(DownloadError::RetryExhausted(request.retry));
				}
				attempt += 1;
				warn!(
					"server refused range, restart from 0 (attempt {}): status={}",
					attempt, status
				);
				*start_from = 0;
				tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
				continue;
			}

			if !(status.is_success() || is_partial) {
				if attempt >= request.retry {
					return Err(DownloadError::UnexpectedStatus(status));
				}
				attempt += 1;
				warn!("unexpected status {}, retry attempt {}", status, attempt);
				tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
				continue;
			}

			let mut stream = resp.bytes_stream();
			let mut stream_error = None;

			while let Some(chunk_result) = stream.try_next().await.transpose() {
				if cancel.as_ref().map(|c| *c.borrow()).unwrap_or(false) {
					return Err(DownloadError::Cancelled);
				}

				match chunk_result {
					Ok(chunk) => {
						if let Err(e) = file.write_all(&chunk).await {
							stream_error = Some(DownloadError::Io(e));
							break;
						}
						let chunk_len = chunk.len() as u64;
						let current_downloaded =
							downloaded.fetch_add(chunk_len, Ordering::Relaxed) + chunk_len;
						*start_from += chunk_len;

						let now = Instant::now();
						let elapsed = (now - *last_instant).as_secs_f64();
						if elapsed >= PROGRESS_UPDATE_INTERVAL.as_secs_f64() {
							let speed_bps = if elapsed > 0.0 {
								(current_downloaded - *last_downloaded) as f64 / elapsed
							} else {
								0.0
							};

							on_progress(DownloadProgress {
								downloaded: current_downloaded,
								total: None,
								speed_bps,
							});

							*last_instant = now;
							*last_downloaded = current_downloaded;
						}
					}
					Err(e) => {
						stream_error = Some(DownloadError::Http(e));
						break;
					}
				}
			}

			if let Some(err) = stream_error {
				if let Ok(metadata) = file.metadata().await {
					*start_from = metadata.len();
				}

				if attempt >= request.retry {
					return Err(err);
				}
				attempt += 1;
				warn!("stream error {}, retry attempt {}", err, attempt);
				tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
				continue;
			}

			return Ok(());
		}
	}
}

async fn file_matches_checksum(path: &Path, checksum: &Checksum) -> Result<bool, std::io::Error> {
	if !path.exists() {
		return Ok(false);
	}

	match checksum {
		Checksum::Sha256(expected) => {
			let path = path.to_owned();
			let expected = expected.clone();
			let digest = tokio::task::spawn_blocking(move || -> Result<String, std::io::Error> {
				let data = std::fs::read(&path)?;
				let hash = hex::encode(Sha256::digest(data));
				Ok(hash)
			})
			.await
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))??;
			Ok(digest.eq_ignore_ascii_case(&expected))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tokio::io::{AsyncReadExt, AsyncWriteExt};
	use tokio::net::TcpListener;

	async fn start_test_server(data: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let url = format!("http://{}", addr);

		let handle = tokio::spawn(async move {
			loop {
				let (mut socket, _) = listener.accept().await.unwrap();
				let data = data.clone();
				tokio::spawn(async move {
					let mut buf = [0u8; 1024];
					let n = socket.read(&mut buf).await.unwrap();
					let req = String::from_utf8_lossy(&buf[..n]);

					let mut start = 0usize;
					let mut end = data.len().saturating_sub(1);
					let mut status = "200 OK";

					for line in req.lines() {
						if line.to_lowercase().starts_with("range:") {
							if let Some(range_str) = line.split('=').nth(1) {
								let parts: Vec<&str> = range_str.split('-').collect();
								if let Some(s) =
									parts.get(0).and_then(|v| v.trim().parse::<usize>().ok())
								{
									start = s.min(data.len().saturating_sub(1));
								}
								if let Some(e) =
									parts.get(1).and_then(|v| v.trim().parse::<usize>().ok())
								{
									end = e.min(data.len().saturating_sub(1));
								}
								status = "206 Partial Content";
							}
						}
					}

					let body = &data[start..=end];
					let mut response = format!(
						"HTTP/1.1 {}\r\nAccept-Ranges: bytes\r\nContent-Length: {}\r\n",
						status,
						body.len()
					);
					if status.starts_with("206") {
						response.push_str(&format!(
							"Content-Range: bytes {}-{}/{}\r\n",
							start,
							end,
							data.len()
						));
					}
					response.push_str("\r\n");

					socket.write_all(response.as_bytes()).await.unwrap();
					socket.write_all(body).await.unwrap();
				});
			}
		});

		(url, handle)
	}

	#[tokio::test]
	async fn test_download_basic() {
		let data = b"hello world from hako".to_vec();
		let (url, server_handle) = start_test_server(data.clone()).await;

		let client = DownloadClient::new().unwrap();
		let dir = tempfile::tempdir().unwrap();
		let dest = dir.path().join("file.bin");

		let mut seen_progress = false;
		client
			.download(
				DownloadRequest::new(format!("{}/file.bin", url), &dest),
				|progress| {
					seen_progress = true;
					assert!(progress.downloaded > 0);
				},
				None,
			)
			.await
			.unwrap();

		let content = tokio::fs::read(&dest).await.unwrap();
		assert_eq!(content, data);
		assert!(seen_progress);

		server_handle.abort();
	}

	#[tokio::test]
	async fn test_resume_download() {
		let data: Vec<u8> = (0..2048u32).map(|v| (v % 255) as u8).collect();
		let (url, server_handle) = start_test_server(data.clone()).await;

		let client = DownloadClient::new().unwrap();
		let dir = tempfile::tempdir().unwrap();
		let dest = dir.path().join("resume.bin");
		let temp = dest.with_extension("hako.part");

		tokio::fs::write(&temp, &data[..1024]).await.unwrap();

		let mut hasher = Sha256::new();
		hasher.update(&data);
		let checksum = Checksum::Sha256(hex::encode(hasher.finalize()));

		client
			.download(
				DownloadRequest::new(format!("{}/resume.bin", url), &dest).with_checksum(checksum),
				|_p| {},
				None,
			)
			.await
			.unwrap();

		let content = tokio::fs::read(&dest).await.unwrap();
		assert_eq!(content, data);

		server_handle.abort();
	}
}
