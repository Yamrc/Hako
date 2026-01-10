use crate::launcher::core::state::AppState;
use crate::minecraft::tasks::start::StartGameTask;
use gpui::{Context, Render, Window, div, prelude::*, px, rgb, white};
use gpui_router::NavLink;

pub struct Navbar;

impl Navbar {
	pub fn new(_cx: &mut Context<Self>) -> Self {
		Self
	}

	fn launch_current(&self) {
		let state = AppState::get();
		let Some(inst) = state.current_instance() else {
			return;
		};
		let tm = state.task_manager.clone();
		let ver = inst.version.clone();
		tokio::runtime::Handle::current().spawn(async move {
			let task = StartGameTask { instance: inst };
			match tm.submit_blocking(task).await {
				Ok(mut h) => {
					tracing::info!("启动: {} ({})", ver, h.id);
					tokio::spawn(async move {
						let _ = h.result().await;
					});
				}
				Err(e) => tracing::error!("启动失败: {}", e),
			}
		});
	}
}

impl Render for Navbar {
	fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
		let has_instance = AppState::get().current_instance.lock().unwrap().is_some();

		div()
			.flex()
			.flex_shrink_0()
			.items_center()
			.justify_between()
			.px_4()
			.h(px(50.))
			.bg(rgb(0x141414))
			.border_t_1()
			.border_color(rgb(0x252525))
			.child(div())
			.child(
				div()
					.flex()
					.items_center()
					.gap_1()
					.child(NavLink::new().to("/").child(nav_label("首页")))
					.child(NavLink::new().to("/download").child(nav_label("下载")))
					.child(NavLink::new().to("/instances").child(nav_label("实例")))
					.child(NavLink::new().to("/settings").child(nav_label("设置")))
					.child(
						div()
							.ml_4()
							.px_4()
							.py_2()
							.rounded_md()
							.bg(if has_instance {
								rgb(0x22c55e)
							} else {
								rgb(0x333333)
							})
							.when(has_instance, |d| {
								d.hover(|s| s.bg(rgb(0x16a34a))).cursor_pointer()
							})
							.text_color(rgb(if has_instance { 0xffffff } else { 0x666666 }))
							.text_sm()
							.child("启动")
							.when(has_instance, |d| {
								d.on_mouse_down(
									gpui::MouseButton::Left,
									cx.listener(|this, _, _, _| this.launch_current()),
								)
							}),
					),
			)
	}
}

fn nav_label(text: &str) -> impl IntoElement {
	div()
		.px_4()
		.py_2()
		.rounded_md()
		.text_sm()
		.text_color(rgb(0xaaaaaa))
		.hover(|s| s.bg(rgb(0x252525)).text_color(white()))
		.cursor_pointer()
		.child(text.to_string())
}
