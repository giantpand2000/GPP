use gpui::{
    App, ClickEvent, CursorStyle, Div, IntoElement, SharedString, Stateful, Window, div,
    prelude::*, px,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::theme;

pub const SUBTITLE_SIZES: &[f32] = &[14.0, 18.0, 22.0, 28.0];
pub const VOLUME_PRESETS: &[f64] = &[0.0, 0.25, 0.5, 0.75, 1.0];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub autoplay: bool,
    pub loop_playback: bool,
    pub auto_hide_controls: bool,
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    pub subtitle_enabled: bool,
    pub subtitle_background: bool,
    pub subtitle_size: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autoplay: true,
            loop_playback: false,
            auto_hide_controls: true,
            volume: 1.0,
            muted: false,
            speed: 1.0,
            subtitle_enabled: true,
            subtitle_background: true,
            subtitle_size: 18.0,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        let Ok(bytes) = fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn cycle_subtitle_size(&mut self, larger: bool) {
        let Some(index) = SUBTITLE_SIZES
            .iter()
            .position(|size| (*size - self.subtitle_size).abs() < 0.1)
        else {
            self.subtitle_size = 18.0;
            return;
        };
        let next = if larger {
            (index + 1).min(SUBTITLE_SIZES.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        self.subtitle_size = SUBTITLE_SIZES[next];
    }
}

fn settings_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/gpp/settings.json")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.join(".config/gpp/settings.json")
    }
}

pub fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .pt_4()
        .pb_2()
        .text_xs()
        .text_color(theme::muted())
        .child(text)
}

pub fn setting_row(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .h(px(44.))
        .flex()
        .items_center()
        .justify_between()
        .child(div().text_sm().text_color(theme::white()).child(label))
        .child(control)
}

pub fn toggle(
    id: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(40.))
        .h(px(22.))
        .rounded_full()
        .flex()
        .items_center()
        .cursor(CursorStyle::PointingHand)
        .bg(if on {
            theme::progress()
        } else {
            theme::toggle_off()
        })
        .on_click(on_click)
        .child(
            div()
                .size(px(18.))
                .rounded_full()
                .bg(theme::white())
                .ml(if on { px(20.) } else { px(2.) }),
        )
}

pub fn choice_chip(
    id: String,
    label: impl Into<gpui::SharedString>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .h(px(28.))
        .px_2()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .cursor(CursorStyle::PointingHand)
        .bg(if active {
            theme::icon_hover()
        } else {
            gpui::transparent_black()
        })
        .text_color(if active {
            theme::white()
        } else {
            theme::muted()
        })
        .hover(|style| style.bg(theme::icon_hover()).text_color(theme::white()))
        .on_click(on_click)
        .child(label.into())
}
