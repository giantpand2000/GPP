use gpui::{Hsla, SharedString, rgb};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::settings::Settings;

const SCROLL_SECONDS: f32 = 8.0;
const HOLD_SECONDS: f32 = 4.5;
const LANE_GAP: f32 = 6.0;
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
}

#[derive(Clone, Debug)]
pub struct LayoutItem {
    pub text: SharedString,
    pub x: f32,
    pub y: f32,
    pub color: Hsla,
    pub font_size: f32,
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
    Ok(DanmakuSession {
        comments,
        source_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
    })
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
    if !settings.danmaku_enabled || session.comments.is_empty() {
        return Vec::new();
    }
    let font_size = settings.danmaku_font_size.clamp(12.0, 40.0);
    let opacity = settings.danmaku_opacity.clamp(0.2, 1.0);
    let speed = settings.danmaku_speed.clamp(0.4, 2.5);
    let density = settings.danmaku_density.clamp(0.15, 1.0);
    let scroll_dur = Duration::from_secs_f32((SCROLL_SECONDS / speed).max(2.0));
    let hold_dur = Duration::from_secs_f32(HOLD_SECONDS);
    let window_start = position.saturating_sub(scroll_dur);
    let slice = comments_in_window(&session.comments, window_start, position);

    let top = view_height * 0.04;
    let usable = usable_height(view_height, settings.danmaku_avoid_subtitles);
    let lane_h = (font_size + LANE_GAP).max(14.0);
    let lane_count = ((usable / lane_h).floor() as usize).clamp(1, 48);
    let max_items = ((lane_count as f32 * 3.5 * density).round() as usize).clamp(8, 160);

    let mut scroll_lanes = vec![None; lane_count];
    let mut top_slots = vec![false; (lane_count / 3).max(1)];
    let mut bottom_slots = vec![false; (lane_count / 3).max(1)];
    let mut items = Vec::new();

    for comment in slice {
        if items.len() >= max_items {
            break;
        }
        if comment.text.is_empty() {
            continue;
        }
        let elapsed = position.saturating_sub(comment.time).as_secs_f32();
        match comment.mode {
            Mode::Scroll => {
                let duration = scroll_dur.as_secs_f32();
                if elapsed > duration {
                    continue;
                }
                let text_w = estimate_width(&comment.text, font_size);
                let travel = view_width + text_w + 24.0;
                let x = view_width - (elapsed / duration) * travel;
                let Some(lane) = pick_scroll_lane(&scroll_lanes, x, text_w) else {
                    continue;
                };
                scroll_lanes[lane] = Some((x, text_w));
                items.push(LayoutItem {
                    text: comment.text.clone().into(),
                    x,
                    y: top + lane as f32 * lane_h,
                    color: comment_color(comment.color, opacity),
                    font_size,
                });
            }
            Mode::Top => {
                if elapsed > hold_dur.as_secs_f32() {
                    continue;
                }
                let Some(slot) = top_slots.iter().position(|used| !used) else {
                    continue;
                };
                top_slots[slot] = true;
                let text_w = estimate_width(&comment.text, font_size);
                items.push(LayoutItem {
                    text: comment.text.clone().into(),
                    x: ((view_width - text_w) * 0.5).max(8.0),
                    y: top + slot as f32 * lane_h,
                    color: comment_color(comment.color, opacity),
                    font_size,
                });
            }
            Mode::Bottom => {
                if elapsed > hold_dur.as_secs_f32() {
                    continue;
                }
                let Some(slot) = bottom_slots.iter().position(|used| !used) else {
                    continue;
                };
                bottom_slots[slot] = true;
                let text_w = estimate_width(&comment.text, font_size);
                let y = top + usable - (slot as f32 + 1.0) * lane_h;
                items.push(LayoutItem {
                    text: comment.text.clone().into(),
                    x: ((view_width - text_w) * 0.5).max(8.0),
                    y: y.max(top),
                    color: comment_color(comment.color, opacity),
                    font_size,
                });
            }
        }
    }
    items
}

fn usable_height(view_height: f32, avoid_subtitles: bool) -> f32 {
    let fraction = if avoid_subtitles {
        1.0 - SUBTITLE_RESERVE
    } else {
        0.92
    };
    (view_height * fraction).max(48.0)
}

fn pick_scroll_lane(lanes: &[Option<(f32, f32)>], x: f32, text_w: f32) -> Option<usize> {
    let new_left = x;
    let new_right = x + text_w;
    lanes.iter().position(|slot| match slot {
        None => true,
        Some((other_x, other_w)) => {
            let other_right = other_x + other_w;
            new_right + 24.0 < *other_x || other_right + 24.0 < new_left
        }
    })
}

fn comments_in_window(comments: &[Comment], start: Duration, end: Duration) -> &[Comment] {
    let i = comments.partition_point(|comment| comment.time < start);
    let j = comments.partition_point(|comment| comment.time <= end);
    &comments[i..j]
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

fn comment_color(rgb24: u32, opacity: f32) -> Hsla {
    let mut color: Hsla = rgb(rgb24 & 0x00FF_FFFF).into();
    color.a = opacity;
    color
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
            && (head.contains("\"comments\"") || head.contains("\"danmaku\"")))
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
    let _size = parts.next();
    let color = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0xFFFFFF);
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

#[derive(Deserialize)]
struct JsonComment {
    #[serde(alias = "t", alias = "stime", alias = "time_ms")]
    time: Option<f64>,
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
    let time = item.time.unwrap_or(0.0);
    let seconds = if time > 10_000.0 { time / 1000.0 } else { time };
    Some(Comment {
        time: Duration::from_secs_f64(seconds.max(0.0)),
        text,
        color: item.color.unwrap_or(0xFFFFFF),
        mode: match item.r#type.unwrap_or(1) {
            4 => Mode::Bottom,
            5 => Mode::Top,
            _ => Mode::Scroll,
        },
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
    fn layout_keeps_comments_above_subtitle_band() {
        let session = DanmakuSession {
            comments: vec![Comment {
                time: Duration::from_secs(1),
                text: "hello".into(),
                color: 0xFFFFFF,
                mode: Mode::Scroll,
            }],
            source_name: "t.xml".into(),
        };
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
}
