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
    // Before any threads start: `set_var` is only sound single-threaded.
    #[cfg(unix)]
    shell_path::adopt_login_shell_path();

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

/// Apps launched from Finder, the Dock or a desktop launcher inherit a minimal
/// PATH without Homebrew or cloud CLIs, so `kubectl` and its auth plugins
/// (`aws`, `gcloud`, `kubelogin`, …) would not be found. When not started from
/// a terminal, adopt the PATH the user's login shell would give them.
#[cfg(unix)]
mod shell_path {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    const MARKER: &str = "__CLUSTER_DESKTOP_PATH__";
    const TIMEOUT: Duration = Duration::from_secs(5);

    pub fn adopt_login_shell_path() {
        if std::env::var_os("TERM").is_some() {
            return; // Started from a terminal: PATH is already the user's.
        }
        if let Some(path) = login_shell_path() {
            std::env::set_var("PATH", path);
        }
    }

    fn login_shell_path() -> Option<String> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        // fish keeps PATH as a list, so join it explicitly.
        let print_path = if shell.ends_with("fish") {
            format!("printf '{MARKER}%s{MARKER}' (string join : $PATH)")
        } else {
            format!("printf '{MARKER}%s{MARKER}' \"$PATH\"")
        };
        let child = Command::new(&shell)
            .args(["-l", "-i", "-c", &print_path])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let pid = child.id();

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });
        let output = match rx.recv_timeout(TIMEOUT) {
            Ok(result) => result.ok()?,
            Err(_) => {
                // A shell rc that waits for input must not hang startup.
                let _ = Command::new("kill").arg(pid.to_string()).status();
                return None;
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout.split(MARKER).nth(1)?.trim();
        (!path.is_empty()).then(|| path.to_string())
    }
}
