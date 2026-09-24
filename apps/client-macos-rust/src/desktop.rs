//! Native menu-bar shell. All menu objects stay on the macOS main thread.
use std::sync::Arc;

use futures::FutureExt;
use tokio_util::sync::CancellationToken;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS},
    window::WindowId,
};

use crate::{Cli, DesktopStatus};

enum Event {
    Status(DesktopStatus),
    Menu(MenuEvent),
    Finished(bool),
}

pub fn run(cli: Cli, runtime: &tokio::runtime::Runtime) {
    let event_loop = EventLoop::<Event>::with_user_event()
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false)
        .build()
        .expect("could not create the macOS event loop");
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(Event::Menu(event));
    }));
    let proxy = event_loop.create_proxy();
    let notify = Arc::new(move |status| {
        let _ = proxy.send_event(Event::Status(status));
    });
    let shutdown = CancellationToken::new();
    let backend_shutdown = shutdown.clone();
    let proxy = event_loop.create_proxy();
    let backend = runtime.spawn(async move {
        let result =
            std::panic::AssertUnwindSafe(crate::serve(cli, backend_shutdown, Some(notify)))
                .catch_unwind()
                .await;
        let _ = proxy.send_event(Event::Finished(result.is_ok()));
    });
    let mut shell = Shell {
        tray: None,
        open: MenuItem::new("Open Gday Meetings", false, None),
        status: MenuItem::new("Status: Starting…", false, None),
        logs: MenuItem::new("Show Logs", true, None),
        quit: MenuItem::new("Quit Gday Meetings", true, None),
        url: None,
        stopping: false,
        failed: false,
        shutdown: shutdown.clone(),
    };
    let result = event_loop.run_app(&mut shell);
    shutdown.cancel();
    // Even an event-loop error must not abandon active audio writers.
    runtime.block_on(backend).expect("client runtime failed");
    if let Err(error) = result {
        tracing::error!("Menu bar event loop failed: {error}");
        shell.failed = true;
    }
    if shell.failed {
        std::process::exit(1);
    }
}

struct Shell {
    tray: Option<TrayIcon>,
    open: MenuItem,
    status: MenuItem,
    logs: MenuItem,
    quit: MenuItem,
    url: Option<String>,
    stopping: bool,
    failed: bool,
    shutdown: CancellationToken,
}

impl Shell {
    fn stopping(&mut self) {
        self.stopping = true;
        self.status.set_text("Status: Stopping…");
        self.open.set_enabled(false);
        self.quit.set_enabled(false);
        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some("Gday Meetings — Stopping…"));
        }
    }
}

impl ApplicationHandler<Event> for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }
        let menu = Menu::new();
        let result = (|| -> Result<TrayIcon, Box<dyn std::error::Error>> {
            menu.append_items(&[
                &self.open,
                &self.status,
                &PredefinedMenuItem::separator(),
                &self.logs,
                &PredefinedMenuItem::separator(),
                &self.quit,
            ])?;
            Ok(TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(waveform_icon())
                .with_icon_as_template(true)
                .with_tooltip("Gday Meetings")
                .build()?)
        })();
        match result {
            Ok(tray) => self.tray = Some(tray),
            Err(error) => {
                tracing::error!("Could not create menu bar: {error}");
                self.failed = true;
                self.shutdown.cancel();
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::Status(DesktopStatus::Ready { url, recordings }) if !self.stopping => {
                self.url = url;
                self.open.set_enabled(self.url.is_some());
                let label = if recordings == 0 {
                    "Ready".into()
                } else {
                    format!("Recording ({recordings})")
                };
                self.status.set_text(format!("Status: {label}"));
                if let Some(tray) = &self.tray {
                    let _ = tray.set_tooltip(Some(format!("Gday Meetings — {label}")));
                    tray.set_title(if recordings > 0 { Some("●") } else { None });
                }
            }
            Event::Status(DesktopStatus::Ready { .. }) => {}
            Event::Status(DesktopStatus::Stopping) => self.stopping(),
            Event::Menu(event) if event.id == *self.quit.id() => {
                self.stopping();
                self.shutdown.cancel();
            }
            Event::Menu(event) if event.id == *self.open.id() && !self.stopping => {
                if let Some(url) = &self.url {
                    open(url);
                }
            }
            Event::Menu(event) if event.id == *self.logs.id() => {
                tracing::info!("Opening current client log in Console.app");
                match gday_meetings_client::logging::current_log_file() {
                    Ok(path) => open_with_app(path.as_os_str(), Some("com.apple.Console")),
                    Err(error) => tracing::warn!("Cannot open current log: {error}"),
                }
            }
            Event::Menu(_) => {}
            Event::Finished(success) => {
                self.failed |= !success;
                self.tray.take();
                event_loop.exit();
            }
        }
    }
}

fn open(path: impl AsRef<std::ffi::OsStr>) {
    open_with_app(path, None);
}

fn open_with_app(path: impl AsRef<std::ffi::OsStr>, app: Option<&'static str>) {
    // `open` exits after handing off to Finder/the browser. Wait on a helper
    // thread so the UI remains responsive and no child process is left unreaped.
    let path = path.as_ref().to_owned();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("/usr/bin/open");
        if let Some(app) = app {
            command.args(["-b", app]);
        }
        match command.arg(path).status() {
            Ok(status) if status.success() => {}
            result => tracing::warn!("Could not open menu destination: {result:?}"),
        }
    });
}

fn waveform_icon() -> Icon {
    // A template image follows the system menu bar's light/dark appearance.
    let mut rgba = vec![0; 18 * 18 * 4];
    for (x, height) in [(2, 6), (5, 12), (8, 16), (11, 10), (14, 4)] {
        for y in (18 - height) / 2..(18 + height) / 2 {
            for dx in 0..2 {
                rgba[(y * 18 + x + dx) * 4 + 3] = 255;
            }
        }
    }
    Icon::from_rgba(rgba, 18, 18).expect("valid menu bar icon")
}
