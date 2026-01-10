use crate::launcher::core::state::AppState;
use gpui::{div, prelude::*, rgb};

pub struct InstancesView;

impl InstancesView {
	pub fn render() -> impl IntoElement {
		let state = AppState::get();
		let instances = state.instances.read().unwrap().clone();
		let current_idx = *state.current_instance.lock().unwrap();
		let cluster_path = state.cluster_path();

		div()
			.flex()
			.flex_col()
			.p_4()
			.gap_3()
			.child(
				div()
					.flex()
					.items_center()
					.justify_between()
					.child(div().text_xl().text_color(rgb(0xffffff)).child("实例列表"))
					.child(
						div()
							.text_sm()
							.text_color(rgb(0x888888))
							.child(format!("{}", cluster_path.display())),
					),
			)
			.child(match instances.is_empty() {
				true => div()
					.flex()
					.flex_col()
					.items_center()
					.justify_center()
					.py_8()
					.gap_2()
					.child(
						div()
							.text_color(rgb(0x888888))
							.child("暂无已安装的游戏实例"),
					)
					.child(
						div()
							.text_sm()
							.text_color(rgb(0x666666))
							.child("前往「下载」页面安装游戏版本"),
					)
					.into_any_element(),
				false => div()
					.flex()
					.flex_col()
					.gap_2()
					.children(instances.into_iter().enumerate().map(|(idx, inst)| {
						let is_sel = current_idx == Some(idx);
						let ver = inst.version.clone();
						let path = inst.version_path.display().to_string();

						div()
							.flex()
							.items_center()
							.justify_between()
							.px_3()
							.py_2()
							.rounded_md()
							.bg(if is_sel { rgb(0x1e3a5f) } else { rgb(0x1a1a1a) })
							.border_1()
							.border_color(if is_sel { rgb(0x3b82f6) } else { rgb(0x333333) })
							.hover(|s| s.bg(rgb(0x252525)))
							.cursor_pointer()
							.on_mouse_down(gpui::MouseButton::Left, move |_, _, _| {
								AppState::get().select_instance(Some(idx));
							})
							.child(
								div()
									.flex()
									.flex_col()
									.gap_1()
									.child(
										div()
											.flex()
											.items_center()
											.gap_2()
											.child(div().text_color(rgb(0xffffff)).child(ver))
											.when(is_sel, |d| {
												d.child(
													div()
														.px_2()
														.py_1()
														.rounded_sm()
														.bg(rgb(0x3b82f6))
														.text_color(rgb(0xffffff))
														.text_xs()
														.child("当前"),
												)
											}),
									)
									.child(div().text_sm().text_color(rgb(0x666666)).child(path)),
							)
					}))
					.into_any_element(),
			})
	}
}
