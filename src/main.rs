mod actions;
mod assets;
mod icon;
mod player;
mod settings;
mod subtitles;
mod theme;
mod util;

use actions::*;
use assets::Assets;
use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, Timer, TitlebarOptions, WindowBounds,
    WindowOptions, prelude::*, px, size,
};
use player::Player;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    configure_gstreamer();

    let argv_source =
        std::env::args()
            .nth(1)
            .and_then(|arg| match util::MediaSource::parse(&arg) {
                Ok(source) => Some(source),
                Err(err) => {
                    eprintln!("gpp: {err}");
                    None
                }
            });

    // Finder / `open -a` send file URLs through Apple Events, not argv.
    let pending_opens: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_for_callback = pending_opens.clone();

    let app = Application::new().with_assets(Assets::new());
    app.on_open_urls(move |urls| {
        if let Ok(mut pending) = pending_for_callback.lock() {
            pending.extend(urls);
        }
    });
    app.run(move |cx: &mut App| {
        cx.activate(true);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys(keybindings());
        cx.set_menus(app_menus());
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let queued = pending_opens
            .lock()
            .map(|mut pending| pending.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut sources = util::media_from_open_strings(&queued);
        if sources.is_empty() {
            if let Some(source) = argv_source.clone() {
                sources.push(source);
            }
        }
        let initial = sources.first().cloned();

        let bounds = Bounds::centered(None, size(px(1120.), px(680.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(640.), px(360.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("GPP".into()),
                        appears_transparent: false,
                        ..Default::default()
                    }),
                    app_id: Some("dev.gpp.player".into()),
                    focus: true,
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Player::new(initial, window, cx)),
            )
            .unwrap();

        if sources.len() > 1 {
            let _ = handle.update(cx, |player, _, cx| {
                player.open_sources(sources, cx);
            });
        }

        let pending = pending_opens.clone();
        cx.spawn(async move |cx| {
            loop {
                Timer::after(Duration::from_millis(80)).await;
                let urls = match pending.lock() {
                    Ok(mut pending) => pending.drain(..).collect::<Vec<_>>(),
                    Err(_) => return,
                };
                if urls.is_empty() {
                    continue;
                }
                if handle
                    .update(cx, |player, _, cx| player.open_from_urls(&urls, cx))
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    });
}

fn keybindings() -> Vec<KeyBinding> {
    let open = if cfg!(target_os = "macos") {
        "cmd-o"
    } else {
        "ctrl-o"
    };
    let quit = if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    };
    let fullscreen = if cfg!(target_os = "macos") {
        "cmd-ctrl-f"
    } else {
        "f11"
    };

    vec![
        KeyBinding::new(quit, Quit, None),
        KeyBinding::new(open, OpenFile, None),
        KeyBinding::new("space", PlayPause, None),
        KeyBinding::new("k", PlayPause, None),
        KeyBinding::new("left", SeekBack, None),
        KeyBinding::new("right", SeekForward, None),
        KeyBinding::new("j", SeekBack, None),
        KeyBinding::new("l", SeekForward, None),
        KeyBinding::new("shift-left", SeekBackLarge, None),
        KeyBinding::new("shift-right", SeekForwardLarge, None),
        KeyBinding::new("up", VolumeUp, None),
        KeyBinding::new("down", VolumeDown, None),
        KeyBinding::new("m", ToggleMute, None),
        KeyBinding::new("r", ToggleLoop, None),
        KeyBinding::new("f", ToggleFullscreen, None),
        KeyBinding::new(fullscreen, ToggleFullscreen, None),
        KeyBinding::new("s", CycleSpeed, None),
        KeyBinding::new("c", CycleSubtitles, None),
        KeyBinding::new("n", NextTrack, None),
        KeyBinding::new("p", PrevTrack, None),
        KeyBinding::new("home", Restart, None),
        KeyBinding::new("0", Restart, None),
        KeyBinding::new("escape", ExitFullscreen, None),
        KeyBinding::new("comma", ToggleSettings, None),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-,"
            } else {
                "ctrl-,"
            },
            ToggleSettings,
            None,
        ),
    ]
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "GPP".into(),
            items: vec![
                MenuItem::action("Open…", OpenFile),
                MenuItem::action("Settings", ToggleSettings),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "Playback".into(),
            items: vec![
                MenuItem::action("Play/Pause", PlayPause),
                MenuItem::action("Restart", Restart),
                MenuItem::separator(),
                MenuItem::action("Seek Back 5s", SeekBack),
                MenuItem::action("Seek Forward 5s", SeekForward),
                MenuItem::action("Previous", PrevTrack),
                MenuItem::action("Next", NextTrack),
                MenuItem::separator(),
                MenuItem::action("Mute", ToggleMute),
                MenuItem::action("Loop", ToggleLoop),
                MenuItem::action("Cycle Speed", CycleSpeed),
                MenuItem::action("Cycle Subtitles", CycleSubtitles),
                MenuItem::action("Fullscreen", ToggleFullscreen),
            ],
        },
    ]
}

fn configure_gstreamer() {
    #[cfg(target_os = "macos")]
    {
        let root = PathBuf::from("/Library/Frameworks/GStreamer.framework/Versions/1.0");
        if !root.exists() {
            return;
        }
        let plugin_path = root.join("lib/gstreamer-1.0");
        let scanner = root.join("libexec/gstreamer-1.0/gst-plugin-scanner");
        // SAFETY: called from main() before any other threads start.
        unsafe {
            if std::env::var_os("GST_PLUGIN_SYSTEM_PATH").is_none() {
                std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &plugin_path);
            }
            if std::env::var_os("GST_PLUGIN_SCANNER").is_none() && scanner.exists() {
                std::env::set_var("GST_PLUGIN_SCANNER", scanner);
            }
            if std::env::var_os("GST_REGISTRY_1_0").is_none() {
                if let Some(cache) = dirs_cache() {
                    let _ = std::fs::create_dir_all(&cache);
                    std::env::set_var("GST_REGISTRY_1_0", cache.join("gpp-gstreamer.reg"));
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn dirs_cache() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Caches/gpp"))
}
