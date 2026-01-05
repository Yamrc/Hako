use crate::ui::{app::HakoApp, build_window_options};
use anyhow::Result;
use gpui::{AppContext, Application};

mod account;
mod config;
mod core;
mod game;
mod net;
mod task;
mod ui;

use core::logger;

#[tokio::main]
async fn main() -> Result<()> {
	logger::init();

	Application::new().run(|cx| {
		gpui_router::init(cx);
		cx.activate(true);

		let rt = tokio::runtime::Handle::current();
		rt.spawn(async move {
			core::state::AppState::init().await;
		});

		cx.open_window(build_window_options(cx), |_, cx| {
			cx.new(|cx| HakoApp::new(cx))
		})
		.expect("Open window failed.");
	});

	Ok(())
}
