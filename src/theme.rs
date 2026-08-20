use gpui::{Hsla, rgb, rgba};

pub fn bg() -> Hsla {
    rgb(0x0B0B0E).into()
}

pub fn surface() -> Hsla {
    rgb(0x14141A).into()
}

pub fn surface_hover() -> Hsla {
    rgb(0x22222C).into()
}

pub fn text() -> Hsla {
    rgb(0xF3F3F7).into()
}

pub fn muted() -> Hsla {
    rgb(0x8E8E99).into()
}

pub fn accent() -> Hsla {
    rgb(0x7C9CFF).into()
}

pub fn accent_dim() -> Hsla {
    rgb(0x3E4E82).into()
}

pub fn danger() -> Hsla {
    rgb(0xFF6B6B).into()
}

pub fn bar() -> Hsla {
    rgba(0x101018E8).into()
}

pub fn track() -> Hsla {
    rgba(0xFFFFFF22).into()
}

pub fn white() -> Hsla {
    rgb(0xFFFFFF).into()
}

pub fn overlay() -> Hsla {
    rgba(0x00000066).into()
}
