mod actions;
mod player;
mod theme;
mod util;

use actions::*;
use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, prelude::*, px, size,
};
use player::Player;
use std::path::PathBuf;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    configure_gstreamer();

    let initial = std::env::args()
        .nth(1)
        .and_then(|arg| match util::MediaSource::parse(&arg) {
            Ok(source) => Some(source),
            Err(err) => {
                eprintln!("gpp: {err}");
                None
            }
        });

    Application::new().run(move |cx: &mut App| {
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

        let bounds = Bounds::centered(None, size(px(1120.), px(680.)), cx);
        let initial = initial.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(360.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("GPP".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                app_id: Some("gpp".into()),
                focus: true,
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Player::new(initial, window, cx)),
        )
        .unwrap();
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
        KeyBinding::new("n", NextTrack, None),
        KeyBinding::new("p", PrevTrack, None),
        KeyBinding::new("home", Restart, None),
        KeyBinding::new("0", Restart, None),
        KeyBinding::new("escape", ExitFullscreen, None),
    ]
}

fn app_menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "GPP".into(),
            items: vec![
                MenuItem::action("Open…", OpenFile),
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
