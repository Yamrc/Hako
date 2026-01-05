use crate::core::state::AppState;
use gpui::{div, prelude::*, rgb};

pub struct SettingsView;

impl SettingsView {
	pub fn render() -> impl IntoElement {
		let config = AppState::get().config.get_sync();
		let cluster_path = tokio::task::block_in_place(|| {
			tokio::runtime::Handle::current().block_on(AppState::get().cluster_path())
		});

		div()
			.flex()
			.flex_col()
			.flex_grow()
			.p_4()
			.gap_4()
			.child(div().text_xl().text_color(rgb(0xffffff)).child("设置"))
			.child(Self::render_section(
				"启动器设置",
				div()
					.flex()
					.flex_col()
					.gap_3()
					.child(Self::render_setting_item(
						"游戏目录",
						&cluster_path.display().to_string(),
						"Minecraft 实例存储位置",
					))
					.child(Self::render_setting_item(
						"主题",
						&config.theme,
						"界面主题设置",
					))
					.child(Self::render_setting_item(
						"语言",
						&config.language,
						"界面显示语言",
					)),
			))
			.child(Self::render_section(
				"全局游戏默认设置",
				div()
					.flex()
					.flex_col()
					.gap_3()
					.child(Self::render_setting_item(
						"Java 路径",
						config
							.game
							.java_path
							.as_ref()
							.map(|p| p.display().to_string())
							.as_deref()
							.unwrap_or("自动检测"),
						"游戏使用的 Java 运行时",
					))
					.child(Self::render_setting_item(
						"最大内存",
						&format!("{} MB", config.game.max_memory_mb),
						"JVM 最大内存分配",
					))
					.child(Self::render_setting_item(
						"窗口大小",
						&format!(
							"{} x {}",
							config.game.window_width, config.game.window_height
						),
						"游戏窗口默认尺寸",
					))
					.child(Self::render_setting_item(
						"JVM 参数",
						if config.game.jvm_args.is_empty() {
							"默认"
						} else {
							&config.game.jvm_args
						},
						"额外的 JVM 启动参数",
					)),
			))
			.child(Self::render_section(
				"网络设置",
				div()
					.flex()
					.flex_col()
					.gap_3()
					.child(Self::render_setting_item(
						"下载并发数",
						&config.download_concurrency.to_string(),
						"同时下载的文件数量",
					)),
			))
	}

	fn render_section(title: &str, content: impl IntoElement) -> impl IntoElement {
		div()
			.flex()
			.flex_col()
			.gap_3()
			.p_4()
			.rounded_lg()
			.bg(rgb(0x141414))
			.child(
				div()
					.text_lg()
					.text_color(rgb(0xffffff))
					.mb_2()
					.child(title.to_string()),
			)
			.child(content)
	}

	fn render_setting_item(label: &str, value: &str, desc: &str) -> impl IntoElement {
		div()
			.flex()
			.items_center()
			.justify_between()
			.py_2()
			.border_b_1()
			.border_color(rgb(0x252525))
			.child(
				div()
					.flex()
					.flex_col()
					.gap_1()
					.child(div().text_color(rgb(0xdddddd)).child(label.to_string()))
					.child(
						div()
							.text_xs()
							.text_color(rgb(0x666666))
							.child(desc.to_string()),
					),
			)
			.child(
				div()
					.px_3()
					.py_1()
					.rounded_md()
					.bg(rgb(0x1a1a1a))
					.text_color(rgb(0x888888))
					.text_sm()
					.child(value.to_string()),
			)
	}
}
