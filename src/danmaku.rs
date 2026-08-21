use gpui::{Hsla, SharedString, rgb};
use serde::Deserialize;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::settings::Settings;

/// Matches MutsumiUniverse/Danmakw `alloc.rs` (layout only).
const SCROLL_SECONDS: f32 = 10.0;
const CENTER_SECONDS: f32 = 5.0;
const TOP_PADDING: f32 = 10.0;
const SPACING_FACTOR: f32 = 1.2;
const SUBTITLE_RESERVE: f32 = 0.28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Scroll,
    Top,
    Bottom,
}

#[derive(Clone, Debug)]
pub struct Comment {
    pub time: Duration,
    pub text: String,
    pub color: u32,
    pub mode: Mode,
}

#[derive(Clone, Debug)]
pub struct DanmakuSession {
    pub comments: Vec<Comment>,
    pub source_name: String,
    bake: RefCell<Option<Bake>>,
}

impl DanmakuSession {
    pub fn new(comments: Vec<Comment>, source_name: impl Into<String>) -> Self {
        Self {
            comments,
            source_name: source_name.into(),
            bake: RefCell::new(None),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayoutItem {
    pub id: u64,
    pub text: SharedString,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub color: Hsla,
    pub font_size: f32,
    pub opacity: f32,
}

pub fn is_danmaku_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    if !ext.eq_ignore_ascii_case("xml") && !ext.eq_ignore_ascii_case("json") {
        return false;
    }
    sniff(path).unwrap_or(false)
}

pub fn sidecar(video: &Path) -> Option<PathBuf> {
    let stem = video.file_stem()?.to_string_lossy();
    let parent = video.parent().unwrap_or_else(|| Path::new("."));
    let names = [
        format!("{stem}.danmaku.xml"),
        format!("{stem}.danmaku.json"),
        format!("{stem}.bilibili.xml"),
        format!("{stem}.xml"),
        format!("{stem}.json"),
    ];
    names
        .into_iter()
        .map(|name| parent.join(name))
        .find(|path| path.is_file() && sniff(path).unwrap_or(false))
}

pub fn load(path: &Path) -> Result<DanmakuSession, String> {
    let bytes = fs::read(path).map_err(|err| format!("read danmaku: {err}"))?;
    let mut comments = parse(&bytes)?;
    comments.sort_by_key(|comment| comment.time);
    Ok(DanmakuSession::new(
        comments,
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
    ))
}

pub fn parse(bytes: &[u8]) -> Result<Vec<Comment>, String> {
    let text = decode_bytes(bytes);
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        parse_json(&text)
    } else if trimmed.contains("<d") {
        Ok(parse_bilibili_xml(&text))
    } else {
        Err("unrecognized danmaku file".into())
    }
}

pub fn layout(
    session: &DanmakuSession,
    position: Duration,
    view_width: f32,
    view_height: f32,
    settings: &Settings,
) -> Vec<LayoutItem> {
    if !settings.danmaku_enabled || session.comments.is_empty() || view_width <= 1.0 {
        return Vec::new();
    }
    let font_size = settings.danmaku_font_size.clamp(12.0, 40.0);
    let opacity = settings.danmaku_opacity.clamp(0.2, 1.0);
    let speed = settings.danmaku_speed.clamp(0.4, 2.5);
    let density = settings.danmaku_density.clamp(0.15, 1.0);

    let line_height = (font_size * SPACING_FACTOR).max(14.0);
    let spacing = font_size;
    let layout_h = usable_height(view_height, settings.danmaku_avoid_subtitles);
    let total_rows = ((layout_h - TOP_PADDING) / line_height).floor() as usize;
    let total_rows = total_rows.max(1);
    let scroll_rows = ((total_rows as f32 * density).floor() as usize).max(1);
    let center_rows = (scroll_rows / 5).max(1);
    let alloc = AllocParams {
        view_width,
        font_size,
        speed,
        spacing,
        scroll_rows,
        center_rows,
    };

    let key = BakeKey {
        width: view_width.round() as u32,
        height: layout_h.round() as u32,
        font_size: (font_size * 10.0).round() as u32,
        speed: (speed * 100.0).round() as u32,
        density: (density * 100.0).round() as u32,
        avoid: settings.danmaku_avoid_subtitles,
    };
    {
        let mut bake = session.bake.borrow_mut();
        if bake.as_ref().is_none_or(|baked| baked.key != key) {
            *bake = Some(Bake {
                key,
                slots: bake_slots(&session.comments, &alloc),
            });
        }
    }

    let visible_for = Duration::from_secs_f32((SCROLL_SECONDS / speed).max(CENTER_SECONDS));
    let window_start = position.saturating_sub(visible_for);
    let i = session
        .comments
        .partition_point(|comment| comment.time < window_start);
    let j = session
        .comments
        .partition_point(|comment| comment.time <= position);

    let bake = session.bake.borrow();
    let slots = &bake.as_ref().expect("bake just populated").slots;
    let mut items = Vec::new();
    for idx in i..j {
        let comment = &session.comments[idx];
        let slot = &slots[idx];
        let Some(row) = slot.row else {
            continue;
        };
        if comment.text.is_empty() {
            continue;
        }
        let elapsed = position.saturating_sub(comment.time).as_secs_f32();
        let row = row as usize;
        match comment.mode {
            Mode::Scroll => {
                let vel = (view_width + slot.width) / SCROLL_SECONDS * speed;
                if vel <= f32::EPSILON {
                    continue;
                }
                let x = view_width - elapsed * vel;
                if x > view_width || x + slot.width <= 0.0 {
                    continue;
                }
                items.push(layout_item(
                    idx,
                    slot.text.clone(),
                    x,
                    TOP_PADDING + row as f32 * line_height,
                    comment.color,
                    font_size,
                    opacity,
                    slot.width,
                ));
            }
            Mode::Top => {
                if elapsed > CENTER_SECONDS {
                    continue;
                }
                items.push(layout_item(
                    idx,
                    slot.text.clone(),
                    ((view_width - slot.width) * 0.5).max(0.0),
                    TOP_PADDING + row as f32 * line_height,
                    comment.color,
                    font_size,
                    opacity,
                    slot.width,
                ));
            }
            Mode::Bottom => {
                if elapsed > CENTER_SECONDS {
                    continue;
                }
                items.push(layout_item(
                    idx,
                    slot.text.clone(),
                    ((view_width - slot.width) * 0.5).max(0.0),
                    layout_h - TOP_PADDING - (row as f32 + 1.0) * line_height,
                    comment.color,
                    font_size,
                    opacity,
                    slot.width,
                ));
            }
        }
    }
    items
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BakeKey {
    width: u32,
    height: u32,
    font_size: u32,
    speed: u32,
    density: u32,
    avoid: bool,
}

#[derive(Clone, Debug)]
struct BakedSlot {
    row: Option<u16>,
    width: f32,
    text: SharedString,
}

#[derive(Clone, Debug)]
struct Bake {
    key: BakeKey,
    slots: Vec<BakedSlot>,
}

struct AllocParams {
    view_width: f32,
    font_size: f32,
    speed: f32,
    spacing: f32,
    scroll_rows: usize,
    center_rows: usize,
}

#[derive(Clone)]
struct ScrollPlaced {
    spawn: Duration,
    width: f32,
    vel: f32,
}

fn bake_slots(comments: &[Comment], alloc: &AllocParams) -> Vec<BakedSlot> {
    let hold = Duration::from_secs_f32(CENTER_SECONDS);
    let mut last_scroll = vec![None; alloc.scroll_rows];
    let mut top_placed: Vec<(usize, Duration)> = Vec::new();
    let mut bottom_placed: Vec<(usize, Duration)> = Vec::new();
    let mut slots = Vec::with_capacity(comments.len());

    for comment in comments {
        if comment.text.is_empty() {
            slots.push(BakedSlot {
                row: None,
                width: 0.0,
                text: SharedString::default(),
            });
            continue;
        }
        let text_w = estimate_width(&comment.text, alloc.font_size);
        let text = SharedString::from(comment.text.clone());
        match comment.mode {
            Mode::Scroll => {
                let vel = (alloc.view_width + text_w) / SCROLL_SECONDS * alloc.speed;
                let row = if vel <= f32::EPSILON {
                    None
                } else {
                    find_scroll_row(
                        &last_scroll,
                        alloc.view_width,
                        vel,
                        comment.time,
                        alloc.spacing,
                    )
                };
                if let Some(row) = row {
                    last_scroll[row] = Some(ScrollPlaced {
                        spawn: comment.time,
                        width: text_w,
                        vel,
                    });
                }
                slots.push(BakedSlot {
                    row: row.map(|row| row as u16),
                    width: text_w,
                    text,
                });
            }
            Mode::Top => {
                let row = find_center_row(&top_placed, comment.time, hold, alloc.center_rows);
                if let Some(row) = row {
                    top_placed.push((row, comment.time));
                }
                slots.push(BakedSlot {
                    row: row.map(|row| row as u16),
                    width: text_w,
                    text,
                });
            }
            Mode::Bottom => {
                let row = find_center_row(&bottom_placed, comment.time, hold, alloc.center_rows);
                if let Some(row) = row {
                    bottom_placed.push((row, comment.time));
                }
                slots.push(BakedSlot {
                    row: row.map(|row| row as u16),
                    width: text_w,
                    text,
                });
            }
        }
    }
    slots
}

fn usable_height(view_height: f32, avoid_subtitles: bool) -> f32 {
    let fraction = if avoid_subtitles {
        1.0 - SUBTITLE_RESERVE
    } else {
        1.0
    };
    (view_height * fraction).max(48.0)
}

fn find_scroll_row(
    last_on_row: &[Option<ScrollPlaced>],
    screen_width: f32,
    vel: f32,
    spawn: Duration,
    spacing: f32,
) -> Option<usize> {
    last_on_row.iter().enumerate().find_map(|(row, last)| {
        scroll_row_is_free(last.as_ref(), screen_width, vel, spawn, spacing).then_some(row)
    })
}

fn scroll_row_is_free(
    last: Option<&ScrollPlaced>,
    screen_width: f32,
    vel: f32,
    spawn: Duration,
    spacing: f32,
) -> bool {
    let Some(last) = last else {
        return true;
    };
    if last.vel <= f32::EPSILON {
        return true;
    }
    let elapsed = spawn.saturating_sub(last.spawn).as_secs_f32();
    let last_x = screen_width - elapsed * last.vel;
    let leave_time = (last_x + last.width + spacing) / last.vel;
    let reach_edge_time = screen_width / vel;
    leave_time < reach_edge_time && screen_width > last.width + spacing + last_x
}

fn find_center_row(
    placed: &[(usize, Duration)],
    spawn: Duration,
    hold: Duration,
    max_rows: usize,
) -> Option<usize> {
    let mut occupied = vec![false; max_rows];
    for (row, start) in placed {
        if *start <= spawn && spawn < *start + hold && *row < max_rows {
            occupied[*row] = true;
        }
    }
    occupied.iter().position(|used| !*used)
}

fn estimate_width(text: &str, font_size: f32) -> f32 {
    text.chars()
        .map(|ch| {
            let n = ch as u32;
            if n >= 0x1F300 || (0x2600..=0x27BF).contains(&n) {
                font_size
            } else if n >= 0x2E80 {
                font_size
            } else if ch.is_ascii() {
                font_size * 0.55
            } else {
                font_size * 0.85
            }
        })
        .sum::<f32>()
        + 12.0
}

fn layout_item(
    id: usize,
    text: SharedString,
    x: f32,
    y: f32,
    rgb24: u32,
    font_size: f32,
    opacity: f32,
    width: f32,
) -> LayoutItem {
    LayoutItem {
        id: id as u64,
        text,
        x,
        y,
        width,
        color: comment_color(rgb24),
        font_size,
        opacity,
    }
}

fn comment_color(rgb24: u32) -> Hsla {
    rgb(rgb24 & 0x00FF_FFFF).into()
}

fn sniff(path: &Path) -> Option<bool> {
    let bytes = fs::read(path).ok()?;
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    Some(looks_like_danmaku(&head))
}

fn looks_like_danmaku(head: &str) -> bool {
    let trimmed = head.trim_start();
    trimmed.contains("<d p=")
        || trimmed.contains("<d p='")
        || (trimmed.starts_with('[') && (head.contains("\"text\"") || head.contains("\"msg\"")))
        || (trimmed.starts_with('{')
            && (head.contains("\"comments\"")
                || head.contains("\"danmaku\"")
                || (head.contains("\"p\"") && head.contains("\"m\""))))
}

fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|chunk| Some(u16::from_le_bytes(chunk.try_into().ok()?)))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_bilibili_xml(input: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i + 6 < bytes.len() {
        if bytes[i] == b'<'
            && bytes[i + 1] == b'd'
            && (bytes[i + 2] == b' ' || bytes[i + 2] == b'>')
        {
            if let Some((comment, next)) = parse_d_tag(&input[i..]) {
                comments.push(comment);
                i += next;
                continue;
            }
        }
        i += 1;
    }
    comments
}

fn parse_d_tag(rest: &str) -> Option<(Comment, usize)> {
    let attrs_start = rest.find("p=")?;
    let quote = rest.as_bytes().get(attrs_start + 2)?;
    if *quote != b'"' && *quote != b'\'' {
        return None;
    }
    let q = *quote as char;
    let value_start = attrs_start + 3;
    let value_end = rest[value_start..].find(q)? + value_start;
    let params = &rest[value_start..value_end];
    let after_attrs = rest.find('>')?;
    let text_start = after_attrs + 1;
    let text_end = rest[text_start..].find("</d>")? + text_start;
    let raw_text = &rest[text_start..text_end];
    let text = decode_xml(raw_text).trim().to_string();
    if text.is_empty() {
        return Some((
            Comment {
                time: Duration::ZERO,
                text: String::new(),
                color: 0xFFFFFF,
                mode: Mode::Scroll,
            },
            text_end + 4,
        ));
    }
    let (time, mode, color) = parse_attr_list(params);
    Some((
        Comment {
            time: Duration::from_secs_f64(time),
            text,
            color,
            mode,
        },
        text_end + 4,
    ))
}

/// XML: time,mode,fontsize,color,...
/// Bilibili JSON exports: time,mode,color,extra
fn parse_attr_list(params: &str) -> (f64, Mode, u32) {
    let mut parts = params.split(',');
    let time = parts
        .next()
        .and_then(|part| part.parse::<f64>().ok())
        .unwrap_or(0.0)
        .max(0.0);
    let mode = match parts
        .next()
        .and_then(|part| part.parse::<u8>().ok())
        .unwrap_or(1)
    {
        4 => Mode::Bottom,
        5 => Mode::Top,
        _ => Mode::Scroll,
    };
    let third = parts.next().and_then(|part| part.parse::<u32>().ok());
    let fourth = parts.next().and_then(|part| part.parse::<u32>().ok());
    let color = match third {
        Some(value) if value > 255 => value,
        Some(_) => fourth.unwrap_or(0xFFFFFF),
        None => 0xFFFFFF,
    };
    (time, mode, color)
}

#[derive(Deserialize)]
struct JsonComment {
    #[serde(alias = "t", alias = "stime", alias = "time_ms")]
    time: Option<f64>,
    p: Option<String>,
    #[serde(alias = "msg", alias = "content", alias = "m")]
    text: Option<String>,
    #[serde(alias = "c")]
    color: Option<u32>,
    #[serde(alias = "mode")]
    r#type: Option<u8>,
}

#[derive(Deserialize)]
struct JsonFile {
    #[serde(alias = "danmaku", alias = "data")]
    comments: Option<Vec<JsonComment>>,
}

fn parse_json(input: &str) -> Result<Vec<Comment>, String> {
    if let Ok(list) = serde_json::from_str::<Vec<JsonComment>>(input) {
        return Ok(list.into_iter().filter_map(json_to_comment).collect());
    }
    let wrapped: JsonFile =
        serde_json::from_str(input).map_err(|err| format!("danmaku json: {err}"))?;
    Ok(wrapped
        .comments
        .unwrap_or_default()
        .into_iter()
        .filter_map(json_to_comment)
        .collect())
}

fn json_to_comment(item: JsonComment) -> Option<Comment> {
    let text = item.text?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let (time, mode, color) = if let Some(p) = item.p.as_deref() {
        parse_attr_list(p)
    } else {
        let time = item.time.unwrap_or(0.0);
        let seconds = if time > 10_000.0 { time / 1000.0 } else { time };
        let mode = match item.r#type.unwrap_or(1) {
            4 => Mode::Bottom,
            5 => Mode::Top,
            _ => Mode::Scroll,
        };
        (seconds.max(0.0), mode, item.color.unwrap_or(0xFFFFFF))
    };
    Some(Comment {
        time: Duration::from_secs_f64(time),
        text,
        color: item.color.unwrap_or(color),
        mode,
    })
}

fn decode_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut ent = String::new();
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == ';' {
                break;
            }
            ent.push(next);
            if ent.len() > 12 {
                break;
            }
        }
        match ent.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            rest => {
                if let Some(hex) = rest.strip_prefix("#x").or_else(|| rest.strip_prefix("#X")) {
                    if let Ok(code) = u32::from_str_radix(hex, 16)
                        && let Some(ch) = char::from_u32(code)
                    {
                        out.push(ch);
                        continue;
                    }
                } else if let Some(num) = rest.strip_prefix('#')
                    && let Ok(code) = num.parse::<u32>()
                    && let Some(ch) = char::from_u32(code)
                {
                    out.push(ch);
                    continue;
                }
                out.push('&');
                out.push_str(&ent);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bilibili_xml_and_emoji_entities() {
        let xml = r#"<i><d p="1.5,1,25,16777215,0,0,0,0">hello &#x1F525;</d><d p="2,5,25,255,0,0,0,0">top</d></i>"#;
        let comments = parse(xml.as_bytes()).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].text, "hello 🔥");
        assert_eq!(comments[0].mode, Mode::Scroll);
        assert_eq!(comments[1].mode, Mode::Top);
        assert_eq!(comments[1].color, 255);
    }

    #[test]
    fn parses_json_array() {
        let json = r#"[{"time":12.5,"text":"hi 😀","color":16711680,"mode":1}]"#;
        let comments = parse(json.as_bytes()).unwrap();
        assert_eq!(comments[0].text, "hi 😀");
        assert!((comments[0].time.as_secs_f64() - 12.5).abs() < 0.01);
    }

    #[test]
    fn parses_bilibili_json_p_and_m() {
        let json = r#"{"count":2,"comments":[{"cid":1,"p":"861.48,5,16707842,[BiliBili]aa","m":"hello"},{"cid":2,"p":"10.5,1,16777215,x","m":"later"}]}"#;
        let mut comments = parse(json.as_bytes()).unwrap();
        comments.sort_by_key(|comment| comment.time);
        assert_eq!(comments.len(), 2);
        assert!((comments[0].time.as_secs_f64() - 10.5).abs() < 0.01);
        assert_eq!(comments[0].mode, Mode::Scroll);
        assert_eq!(comments[1].mode, Mode::Top);
        assert_eq!(comments[1].color, 16_707_842);
        assert_eq!(comments[1].text, "hello");
    }

    #[test]
    fn parses_ass_video_json_fixture() {
        let path = Path::new("/tmp/ass-video.json");
        if !path.exists() {
            return;
        }
        let session = load(path).expect("load fixture");
        assert!(session.comments.len() > 1000);
        assert!(
            session
                .comments
                .iter()
                .any(|comment| comment.time > Duration::from_secs(60)),
            "all comments were parked at t=0"
        );
    }

    #[test]
    fn layout_keeps_comments_above_subtitle_band() {
        let session = DanmakuSession::new(
            vec![Comment {
                time: Duration::from_secs(1),
                text: "hello".into(),
                color: 0xFFFFFF,
                mode: Mode::Scroll,
            }],
            "t.xml",
        );
        let settings = Settings {
            danmaku_enabled: true,
            danmaku_avoid_subtitles: true,
            danmaku_font_size: 20.0,
            danmaku_opacity: 1.0,
            danmaku_speed: 1.0,
            danmaku_density: 1.0,
            ..Settings::default()
        };
        let items = layout(&session, Duration::from_secs(2), 800.0, 400.0, &settings);
        assert!(!items.is_empty());
        let max_y = items.iter().map(|item| item.y).fold(0.0_f32, f32::max);
        assert!(
            max_y < 400.0 * 0.8,
            "danmaku leaked into subtitle band: {max_y}"
        );
    }

    #[test]
    fn later_comments_still_layout() {
        let session = DanmakuSession::new(
            vec![
                Comment {
                    time: Duration::from_secs(1),
                    text: "early".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
                Comment {
                    time: Duration::from_secs(120),
                    text: "late".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
            ],
            "t.json",
        );
        let settings = Settings {
            danmaku_enabled: true,
            danmaku_density: 1.0,
            ..Settings::default()
        };
        let items = layout(&session, Duration::from_secs(121), 800.0, 400.0, &settings);
        assert!(
            items.iter().any(|item| item.text.as_ref() == "late"),
            "expected the t=120 comment at t=121"
        );
        assert!(
            items.iter().all(|item| item.text.as_ref() != "early"),
            "t=1 comment should have left the screen"
        );
    }

    #[test]
    fn simultaneous_scrolls_use_separate_rows() {
        let session = DanmakuSession::new(
            vec![
                Comment {
                    time: Duration::from_secs(1),
                    text: "aaaaaaaa".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
                Comment {
                    time: Duration::from_secs(1),
                    text: "bbbbbbbb".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
            ],
            "t.json",
        );
        let settings = Settings {
            danmaku_enabled: true,
            danmaku_density: 1.0,
            danmaku_avoid_subtitles: false,
            ..Settings::default()
        };
        let items = layout(&session, Duration::from_secs(1), 800.0, 400.0, &settings);
        assert_eq!(items.len(), 2);
        assert_ne!(items[0].y, items[1].y);
    }

    #[test]
    fn layout_keeps_fill_opaque_and_carries_opacity() {
        let session = DanmakuSession::new(
            vec![Comment {
                time: Duration::from_secs(1),
                text: "fade".into(),
                color: 0xFFFFFF,
                mode: Mode::Scroll,
            }],
            "t.json",
        );
        let settings = Settings {
            danmaku_enabled: true,
            danmaku_opacity: 0.5,
            danmaku_density: 1.0,
            ..Settings::default()
        };
        let items = layout(&session, Duration::from_secs(2), 800.0, 400.0, &settings);
        assert_eq!(items.len(), 1);
        assert!((items[0].opacity - 0.5).abs() < f32::EPSILON);
        assert!((items[0].color.a - 1.0).abs() < f32::EPSILON);
        assert!(items[0].width > 0.0);
    }

    #[test]
    fn scroll_row_stays_put_after_earlier_comments_expire() {
        let session = DanmakuSession::new(
            vec![
                Comment {
                    time: Duration::ZERO,
                    text: "block-aaaaaaaa".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
                Comment {
                    time: Duration::from_millis(250),
                    text: "marker".into(),
                    color: 0xFFFFFF,
                    mode: Mode::Scroll,
                },
            ],
            "t.json",
        );
        let settings = Settings {
            danmaku_enabled: true,
            danmaku_density: 1.0,
            danmaku_speed: 1.0,
            danmaku_avoid_subtitles: false,
            ..Settings::default()
        };
        let early = layout(&session, Duration::from_secs(1), 800.0, 400.0, &settings);
        let late = layout(&session, Duration::from_millis(10_100), 800.0, 400.0, &settings);
        let y_early = early
            .iter()
            .find(|item| item.text.as_ref() == "marker")
            .map(|item| item.y);
        let y_late = late
            .iter()
            .find(|item| item.text.as_ref() == "marker")
            .map(|item| item.y);
        assert!(y_early.is_some(), "marker should be on screen at t=3");
        assert_eq!(
            y_early, y_late,
            "row must not jump after the t=0 comment leaves the 10s window"
        );
        let blocker_y = early
            .iter()
            .find(|item| item.text.as_ref() != "marker")
            .map(|item| item.y);
        assert_ne!(
            y_early, blocker_y,
            "marker should have been assigned a different row at spawn"
        );
    }
}
