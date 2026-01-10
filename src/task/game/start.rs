use crate::config::manager::ConfigManager;
use crate::core::state::AppState;
use crate::game::args::{Features, collect_game_args, collect_jvm_args};
use crate::game::classpath::build_classpath;
use crate::game::instance::GameInstance;
use crate::game::java::find_java;
use crate::game::natives::{extract_natives, get_natives_directory};
use crate::game::profile::{VersionProfile, load_version_profile};
use crate::task::error::{TaskError, TaskResult};
use crate::task::lock::LockKey;
use crate::task::main_task::{BlockingTask, TaskContext, TaskType};
use crate::task::sub_task::{SubTask, SubTaskChain, SubTaskContext};
use anyhow::Context;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::RwLock;

const MAX_WAIT_TIME: Duration = Duration::from_secs(30);

struct StartContext {
	game_dir: PathBuf,
	version_id: String,
	java_path: Option<PathBuf>,
	max_memory_mb: u32,
	extra_jvm_args: Vec<String>,
	extra_game_args: Vec<String>,

	profile: Option<VersionProfile>,
	natives_dir: Option<PathBuf>,
	java_bin: Option<PathBuf>,
	classpath: Option<String>,
	jvm_args: Vec<String>,
	game_args: Vec<String>,
	username: String,
	uuid: String,
}

impl StartContext {
	fn from_instance(instance: &GameInstance) -> Self {
		let state = AppState::get();
		let launcher_config = state.config.get();
		let game_config =
			ConfigManager::load_game_config(&instance.cluster_path, &instance.version);
		let resolved = game_config.resolve(&launcher_config.game);

		let (username, uuid) = state
			.accounts
			.current()
			.map(|a| (a.username().to_string(), a.uuid().to_string()))
			.unwrap_or_else(|| {
				(
					"Player".into(),
					crate::account::offline_uuid("Player").to_string(),
				)
			});

		let jvm_args: Vec<String> = resolved
			.jvm_args
			.split_whitespace()
			.map(String::from)
			.collect();
		let game_args: Vec<String> = resolved
			.game_args
			.split_whitespace()
			.map(String::from)
			.collect();

		Self {
			game_dir: instance.cluster_path.clone(),
			version_id: instance.version.clone(),
			java_path: resolved.java_path,
			max_memory_mb: resolved.max_memory_mb,
			extra_jvm_args: jvm_args,
			extra_game_args: game_args,
			profile: None,
			natives_dir: None,
			java_bin: None,
			classpath: None,
			jvm_args: Vec::new(),
			game_args: Vec::new(),
			username,
			uuid,
		}
	}
}

pub struct StartGameTask {
	pub instance: GameInstance,
}

impl TaskType for StartGameTask {
	const TYPE_NAME: &'static str = "start_game";
}

#[async_trait::async_trait]
impl BlockingTask for StartGameTask {
	type Output = ();

	fn locks(&self) -> Vec<LockKey> {
		vec![LockKey::global("start_game")]
	}

	fn queueable(&self) -> bool {
		false
	}

	async fn execute(&mut self, ctx: &TaskContext) -> TaskResult<Self::Output> {
		let shared = Arc::new(RwLock::new(StartContext::from_instance(&self.instance)));

		let mut chain = SubTaskChain::new();
		chain.add(PrepareEnvTask(Arc::clone(&shared)));
		chain.add(LaunchTask(shared));

		let sub_ctx = SubTaskContext::new(ctx.cancelled_receiver());
		chain.execute(&sub_ctx).await
	}
}

struct PrepareEnvTask(Arc<RwLock<StartContext>>);

#[async_trait::async_trait]
impl SubTask for PrepareEnvTask {
	async fn execute(&self, _ctx: &SubTaskContext) -> Result<(), TaskError> {
		let mut s = self.0.write().await;

		let profile = load_version_profile(&s.game_dir, &s.version_id)
			.map_err(|e| TaskError::Failed(format!("load profile: {e}")))?;

		let natives_dir = get_natives_directory(&s.game_dir, &s.version_id)
			.map_err(|e| TaskError::Failed(e.to_string()))?;

		let features = Features::default();
		extract_natives(&s.game_dir, &profile, &natives_dir, &features)
			.map_err(|e| TaskError::Failed(e.to_string()))?;

		let java_bin =
			find_java(s.java_path.take()).map_err(|e| TaskError::Failed(e.to_string()))?;

		let cp = build_classpath(&s.game_dir, &s.version_id, &profile, &features)
			.map_err(|e| TaskError::Failed(e.to_string()))?;

		let assets_index = profile
			.assets
			.as_deref()
			.unwrap_or(&s.version_id)
			.to_string();

		let mut jvm_args = collect_jvm_args(
			&profile,
			&s.game_dir,
			&s.version_id,
			&cp,
			&assets_index,
			&s.username,
			&s.uuid,
			&natives_dir,
			&features,
		);

		jvm_args.insert(0, format!("-Xmx{}M", s.max_memory_mb));

		let game_args = collect_game_args(
			&s.game_dir,
			&s.version_id,
			&profile,
			&s.username,
			&s.uuid,
			&assets_index,
			&features,
		);

		s.profile = Some(profile);
		s.natives_dir = Some(natives_dir);
		s.java_bin = Some(java_bin);
		s.classpath = Some(cp);
		s.jvm_args = jvm_args;
		let extra_jvm = std::mem::take(&mut s.extra_jvm_args);
		s.jvm_args.extend(extra_jvm);
		s.game_args = game_args;
		let extra_game = std::mem::take(&mut s.extra_game_args);
		s.game_args.extend(extra_game);

		Ok(())
	}
}

struct LaunchTask(Arc<RwLock<StartContext>>);

#[async_trait::async_trait]
impl SubTask for LaunchTask {
	async fn execute(&self, ctx: &SubTaskContext) -> Result<(), TaskError> {
		let s = self.0.read().await;

		let profile = s
			.profile
			.as_ref()
			.ok_or_else(|| TaskError::Failed("profile missing".into()))?;
		let java_bin = s
			.java_bin
			.as_ref()
			.ok_or_else(|| TaskError::Failed("java missing".into()))?;
		let main_class = profile
			.main_class
			.as_ref()
			.ok_or_else(|| TaskError::Failed("mainClass missing".into()))?;

		let mut cmd = Command::new(java_bin);

		#[cfg(windows)]
		{
			const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
			cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
		}

		cmd.args(&s.jvm_args)
			.arg(main_class)
			.args(&s.game_args)
			.current_dir(&s.game_dir)
			.stdin(std::process::Stdio::null())
			.stdout(std::process::Stdio::piped())
			.stderr(std::process::Stdio::piped());

		let mut child = cmd
			.spawn()
			.context("spawn game process")
			.map_err(|e| TaskError::Failed(e.to_string()))?;

		drop(s);

		let stdout = child
			.stdout
			.take()
			.ok_or_else(|| TaskError::Failed("no stdout".into()))?;
		let stderr = child
			.stderr
			.take()
			.ok_or_else(|| TaskError::Failed("no stderr".into()))?;

		let mut stdout_lines = tokio::io::BufReader::new(stdout).lines();
		let mut stderr_lines = tokio::io::BufReader::new(stderr).lines();
		let mut cancelled = ctx.cancelled.clone();
		let start = Instant::now();

		loop {
			tokio::select! {
				line = stdout_lines.next_line() => {
					if let Ok(Some(log)) = line {
						if is_game_initialized(&log) {
							tracing::info!("game initialized");
							return Ok(());
						}
					}
				}
				line = stderr_lines.next_line() => {
					if let Ok(Some(log)) = line {
						if is_game_initialized(&log) {
							tracing::info!("game initialized");
							return Ok(());
						}
					}
				}
				_ = tokio::time::sleep(Duration::from_millis(100)) => {
					if let Ok(Some(status)) = child.try_wait() {
						if !status.success() {
							return Err(TaskError::Failed(format!("Game exited: {:?}", status.code())));
						}
						return Ok(());
					}
					if start.elapsed() > MAX_WAIT_TIME {
						tracing::warn!("timeout, assuming game started");
						return Ok(());
					}
				}
				_ = cancelled.changed() => {
					let _ = child.kill().await;
					return Err(TaskError::Cancelled);
				}
			}
		}
	}
}

fn is_game_initialized(log: &str) -> bool {
	let l = log.to_lowercase();
	l.contains("lwjgl version") || l.contains("openal initialized") || l.contains("setting user:")
}
