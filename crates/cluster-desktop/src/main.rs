//! cluster-desktop: a GPUI companion to the `cluster` TUI. It watches every
//! kubectl context at once through the same read-only core the TUI uses.

mod backend;
mod theme;
mod view;

use cluster_core::config::{Config, ConfigOverrides};
use gpui::{
    px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};

fn main() {
    let config = Config::load(ConfigOverrides::default()).unwrap_or_default();
    let (backend, events) = backend::Backend::start(config);

    Application::new().run(move |cx: &mut App| {
        theme::load_fonts(cx);

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("cluster".into()),
                    ..Default::default()
                }),
                app_id: Some("cluster-desktop".into()),
                window_min_size: Some(size(px(960.), px(640.))),
                ..Default::default()
            },
            |window, cx| {
                // The titlebar title is only applied on macOS; Linux needs it set here.
                window.set_window_title("cluster");
                cx.new(|cx| view::DesktopApp::new(backend, events, window, cx))
            },
        )
        .expect("failed to open the main window");

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
