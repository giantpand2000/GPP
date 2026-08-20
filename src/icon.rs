use gpui::{
    App, ClickEvent, CursorStyle, Div, IntoElement, Stateful, Window, div, prelude::*, px, svg,
};

use crate::theme;

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    SkipNext,
    Replay,
    Forward,
    VolumeUp,
    VolumeDown,
    VolumeOff,
    Fullscreen,
    FullscreenExit,
    Repeat,
    Folder,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Self::Play => "icons/play.svg",
            Self::Pause => "icons/pause.svg",
            Self::SkipNext => "icons/skip_next.svg",
            Self::Replay => "icons/replay.svg",
            Self::Forward => "icons/forward.svg",
            Self::VolumeUp => "icons/volume_up.svg",
            Self::VolumeDown => "icons/volume_down.svg",
            Self::VolumeOff => "icons/volume_off.svg",
            Self::Fullscreen => "icons/fullscreen.svg",
            Self::FullscreenExit => "icons/fullscreen_exit.svg",
            Self::Repeat => "icons/repeat.svg",
            Self::Folder => "icons/folder.svg",
        }
    }
}

pub fn icon(kind: Icon, size: f32) -> impl IntoElement {
    svg()
        .path(kind.path())
        .size(px(size))
        .flex_none()
        .text_color(theme::white())
}

pub fn icon_button(
    id: &'static str,
    kind: Icon,
    active: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = if active {
        theme::progress()
    } else if disabled {
        theme::muted()
    } else {
        theme::white()
    };
    icon_control(
        id,
        disabled,
        on_click,
        svg().path(kind.path()).size(px(24.)).text_color(color),
    )
}

pub fn skip_button(
    id: &'static str,
    kind: Icon,
    seconds: &'static str,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = if disabled {
        theme::muted()
    } else {
        theme::white()
    };
    icon_control(
        id,
        disabled,
        on_click,
        div()
            .relative()
            .size(px(28.))
            .child(
                svg()
                    .path(kind.path())
                    .size(px(26.))
                    .text_color(color)
                    .absolute()
                    .top(px(1.))
                    .left(px(1.)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .pt(px(2.))
                    .text_color(color)
                    .text_xs()
                    .child(seconds),
            ),
    )
}

fn icon_control(
    id: &'static str,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    glyph: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex_none()
        .w(px(40.))
        .h(px(40.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .when(!disabled, |this| {
            this.cursor(CursorStyle::PointingHand)
                .hover(|style| style.bg(theme::icon_hover()))
                .active(|style| style.opacity(0.7))
                .on_click(on_click)
        })
        .when(disabled, |this| this.opacity(0.38))
        .child(glyph)
}

pub fn text_button(
    id: &'static str,
    label: impl Into<gpui::SharedString>,
    active: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(40.))
        .px_2()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(if active {
            theme::progress()
        } else if disabled {
            theme::muted()
        } else {
            theme::white()
        })
        .when(!disabled, |this| {
            this.cursor(CursorStyle::PointingHand)
                .hover(|style| style.bg(theme::icon_hover()))
                .active(|style| style.opacity(0.7))
                .on_click(on_click)
        })
        .when(disabled, |this| this.opacity(0.38))
        .child(label.into())
}
