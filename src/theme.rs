use gpui::{Hsla, linear_color_stop, linear_gradient, rgb, rgba};

pub fn bg() -> Hsla {
    rgb(0x0F0F0F).into()
}

pub fn text() -> Hsla {
    rgb(0xFFFFFF).into()
}

pub fn muted() -> Hsla {
    rgb(0xAAAAAA).into()
}

pub fn white() -> Hsla {
    rgb(0xFFFFFF).into()
}

pub fn progress() -> Hsla {
    rgb(0xFF0000).into()
}

pub fn progress_track() -> Hsla {
    rgba(0xFFFFFF4D).into()
}

pub fn seek_tooltip() -> Hsla {
    rgba(0x000000CC).into()
}

pub fn icon_hover() -> Hsla {
    rgba(0xFFFFFF22).into()
}

pub fn danger() -> Hsla {
    rgb(0xFF4E45).into()
}

pub fn overlay() -> Hsla {
    rgba(0x00000080).into()
}

pub fn settings_veil() -> Hsla {
    rgba(0x000000B8).into()
}

pub fn settings_panel() -> Hsla {
    rgba(0x212121F5).into()
}

pub fn toggle_off() -> Hsla {
    rgba(0xFFFFFF38).into()
}

pub fn settings_rule() -> Hsla {
    rgba(0xFFFFFF14).into()
}

pub fn bottom_gradient() -> gpui::Background {
    linear_gradient(
        180.,
        linear_color_stop(rgba(0x00000000), 0.),
        linear_color_stop(rgba(0x000000CC), 1.),
    )
}

pub fn top_gradient() -> gpui::Background {
    linear_gradient(
        0.,
        linear_color_stop(rgba(0x00000000), 0.),
        linear_color_stop(rgba(0x000000B3), 1.),
    )
}
