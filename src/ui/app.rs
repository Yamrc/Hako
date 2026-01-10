use crate::launcher::core::state::AppState;
use crate::ui::components::{navbar::Navbar, topbar::Topbar};
use crate::ui::views::{
	download::DownloadView, home::HomeView, instances::InstancesView, settings::SettingsView,
	tasks::TasksView,
};
use gpui::{Context, Entity, Render, Window, div, prelude::*, rgb};
use gpui_router::{Route, Routes};

pub struct HakoApp {
	topbar: Entity<Topbar>,
	navbar: Entity<Navbar>,
}

impl HakoApp {
	pub fn new(ctx: &mut Context<Self>) -> Self {
		AppState::init();

		Self {
			topbar: ctx.new(|_| Topbar::new()),
			navbar: ctx.new(|cx| Navbar::new(cx)),
		}
	}
}

impl Render for HakoApp {
	fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
		div()
			.flex()
			.flex_col()
			.bg(rgb(0x0a0a0a))
			.size_full()
			.child(self.topbar.clone())
			.child(
				div()
					.id("main-content")
					.flex_grow()
					.min_h_0()
					.overflow_y_scroll()
					.child(
						Routes::new()
							.basename("/")
							.child(Route::new().index().element(|_, _| HomeView::render()))
							.child(
								Route::new()
									.path("download")
									.element(|_, _| DownloadView::render()),
							)
							.child(
								Route::new()
									.path("instances")
									.element(|_, _| InstancesView::render()),
							)
							.child(
								Route::new()
									.path("tasks")
									.element(|_, _| TasksView::render()),
							)
							.child(
								Route::new()
									.path("settings")
									.element(|_, _| SettingsView::render()),
							),
					),
			)
			.child(self.navbar.clone())
	}
}
