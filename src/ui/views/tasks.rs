use crate::launcher::core::state::AppState;
use crate::minecraft::tasks::download::{DownloadProgressState, ProgressRef};
use crate::launcher::task::handle::TaskId;
use gpui::{div, prelude::*, px, rgb};

pub struct TasksView;

impl TasksView {
	pub fn render() -> impl IntoElement {
		let state = AppState::get();
		let tasks: Vec<_> = state
			.task_progress
			.lock()
			.unwrap()
			.iter()
			.map(|(id, p)| (*id, p.clone()))
			.collect();

		div()
			.flex()
			.flex_col()
			.flex_grow()
			.p_4()
			.gap_3()
			.child(
				div()
					.flex()
					.items_center()
					.justify_between()
					.child(div().text_xl().text_color(rgb(0xffffff)).child("任务列表"))
					.child(
						div()
							.text_sm()
							.text_color(rgb(0x888888))
							.child(format!("共 {} 个任务", tasks.len())),
					),
			)
			.child(if tasks.is_empty() {
				div()
					.flex()
					.items_center()
					.justify_center()
					.py_8()
					.child(div().text_color(rgb(0x888888)).child("暂无进行中的任务"))
					.into_any_element()
			} else {
				div()
					.flex()
					.flex_col()
					.gap_2()
					.children(
						tasks
							.into_iter()
							.map(|(id, p)| Self::render_task_item(id, p)),
					)
					.into_any_element()
			})
	}

	fn render_task_item(task_id: TaskId, progress: ProgressRef) -> impl IntoElement {
		let p = {
			let rt = tokio::runtime::Handle::current();
			rt.block_on(async { progress.read().await.clone() })
		};
		let percent = p
			.total
			.map(|t| {
				if t > 0 {
					(p.downloaded * 100 / t) as u32
				} else {
					0
				}
			})
			.unwrap_or(0);
		let speed_text = if p.speed_bps > 0.0 {
			format!("{:.1} KB/s", p.speed_bps / 1024.0)
		} else {
			"-".into()
		};
		let size_text = p
			.total
			.map(|t| {
				format!(
					"{:.1} / {:.1} MB",
					p.downloaded as f64 / 1024.0 / 1024.0,
					t as f64 / 1024.0 / 1024.0
				)
			})
			.unwrap_or_else(|| format!("{:.1} MB", p.downloaded as f64 / 1024.0 / 1024.0));

		let task_manager = AppState::get().task_manager.clone();

		div()
			.flex()
			.flex_col()
			.gap_2()
			.px_3()
			.py_3()
			.rounded_md()
			.bg(rgb(0x1a1a1a))
			.child(
				div()
					.flex()
					.items_center()
					.justify_between()
					.child(
						div()
							.flex()
							.items_center()
							.gap_2()
							.child(div().w(px(8.)).h(px(8.)).rounded_full().bg(if p.finished {
								rgb(0x22c55e)
							} else {
								rgb(0x3b82f6)
							}))
							.child(div().text_color(rgb(0xffffff)).child(p.message.clone())),
					)
					.child(
						div()
							.flex()
							.items_center()
							.gap_3()
							.child(div().text_sm().text_color(rgb(0x888888)).child(speed_text))
							.child(if !p.finished {
								div()
									.px_2()
									.py_1()
									.rounded_sm()
									.bg(rgb(0xef4444))
									.hover(|s| s.bg(rgb(0xdc2626)))
									.cursor_pointer()
									.text_color(rgb(0xffffff))
									.text_xs()
									.child("取消")
									.on_mouse_down(gpui::MouseButton::Left, {
										let tm = task_manager;
										move |_, _, _cx| {
											let tm = tm.clone();
											let tid = task_id;
											tokio::runtime::Handle::current().spawn(async move {
												if let Err(e) = tm.cancel(tid).await {
													tracing::error!("取消任务失败: {}", e);
												}
											});
										}
									})
									.into_any_element()
							} else {
								div()
									.px_2()
									.py_1()
									.rounded_sm()
									.bg(rgb(0x22c55e))
									.text_color(rgb(0xffffff))
									.text_xs()
									.child("完成")
									.into_any_element()
							}),
					),
			)
			.child(
				div()
					.flex()
					.items_center()
					.gap_3()
					.child(
						div()
							.flex_grow()
							.h(px(4.))
							.rounded_full()
							.bg(rgb(0x333333))
							.child(
								div()
									.h_full()
									.rounded_full()
									.bg(if p.finished {
										rgb(0x22c55e)
									} else {
										rgb(0x3b82f6)
									})
									.w(gpui::relative(percent as f32 / 100.0)),
							),
					)
					.child(
						div()
							.text_xs()
							.text_color(rgb(0x888888))
							.child(format!("{}%", percent)),
					),
			)
			.child(div().text_xs().text_color(rgb(0x666666)).child(format!(
				"ID: {} | {}",
				&task_id.to_string()[..8],
				size_text
			)))
	}
}
