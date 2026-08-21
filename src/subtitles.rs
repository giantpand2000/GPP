use gpui_video_player::gst;
use gpui_video_player::{Error as VideoError, Video, VideoOptions};
use gst::glib::FlagsClass;
use gst::prelude::*;
use gstreamer_app as gst_app;
use std::path::Path;
use url::Url;

use crate::util;

#[derive(Clone, Debug)]
pub struct SubtitleTrack {
    pub index: i32,
    pub label: String,
}

pub struct SubtitleSession {
    pipeline: gst::Pipeline,
    tracks: Vec<SubtitleTrack>,
    current: Option<i32>,
}

pub struct OpenedMedia {
    pub video: Video,
    pub subtitles: SubtitleSession,
}

pub fn open(
    uri: &Url,
    sidecar: Option<&Url>,
    options: VideoOptions,
) -> Result<OpenedMedia, String> {
    gst::init().map_err(|err| err.to_string())?;

    let pipeline_desc = format!(
        "playbin uri=\"{}\" video-sink=\"videoscale ! videoconvert ! appsink name=gpui_video drop=true max-buffers=200 enable-last-sample=false caps=video/x-raw,format=NV12,pixel-aspect-ratio=1/1\"",
        escape_launch_uri(uri.as_str())
    );
    let pipeline = gst::parse::launch(&pipeline_desc)
        .map_err(|err| err.to_string())?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "failed to create playbin pipeline".to_string())?;

    // Leave text-sink unset so playbin inserts subtitleoverlay, which autoplugs
    // assrender (libass) for application/x-ass and SSA tracks.
    if let Some(sidecar) = sidecar {
        pipeline.set_property("suburi", sidecar.as_str());
    }

    let video_sink = video_appsink(&pipeline)?;
    let video = Video::from_gst_pipeline_with_options(pipeline.clone(), video_sink, None, options)
        .map_err(|err: VideoError| err.to_string())?;

    let mut subtitles = SubtitleSession {
        pipeline,
        tracks: Vec::new(),
        current: None,
    };
    subtitles.refresh_tracks();
    Ok(OpenedMedia { video, subtitles })
}

impl SubtitleSession {
    pub fn refresh_tracks(&mut self) {
        let count = self.pipeline.property::<i32>("n-text").max(0);
        // playbin can report n-text=0 while text is disabled or before
        // preroll. Keep the last known list so Off → first-track still works.
        if count <= 0 {
            return;
        }
        let first_discovery = self.tracks.is_empty();
        self.tracks = (0..count)
            .map(|index| SubtitleTrack {
                index,
                label: track_label(&self.pipeline, index),
            })
            .collect();
        if first_discovery {
            let current = self.pipeline.property::<i32>("current-text");
            self.current = if current >= 0 { Some(current) } else { None };
        } else if let Some(current) = self.current
            && !self.tracks.iter().any(|track| track.index == current)
        {
            self.current = self.tracks.first().map(|track| track.index);
        }
    }

    pub fn tracks(&self) -> &[SubtitleTrack] {
        &self.tracks
    }

    pub fn current(&self) -> Option<i32> {
        self.current
    }

    pub fn current_label(&self) -> Option<&str> {
        let current = self.current?;
        self.tracks
            .iter()
            .find(|track| track.index == current)
            .map(|track| track.label.as_str())
    }

    pub fn set_current(&mut self, index: Option<i32>) {
        self.current = index;
        match index {
            Some(index) => {
                set_play_flag_text(&self.pipeline, true);
                self.pipeline.set_property("current-text", index);
                set_overlay_visible(&self.pipeline, true);
            }
            None => {
                set_overlay_visible(&self.pipeline, false);
                self.pipeline.set_property("current-text", -1i32);
                set_play_flag_text(&self.pipeline, false);
            }
        }
    }

    pub fn apply_font_size(&self, size: f32) {
        let desc = format!("Sans {}", size.round().clamp(10.0, 48.0) as i32);
        self.pipeline.set_property("subtitle-font-desc", desc);
    }

    pub fn cycle(&mut self) -> String {
        self.refresh_tracks();
        if self.tracks.is_empty() {
            self.set_current(None);
            return "Subtitles off".into();
        }
        let next = next_subtitle_selection(&self.tracks, self.current);
        self.set_current(next);
        match next {
            Some(index) => self
                .tracks
                .iter()
                .find(|track| track.index == index)
                .map(|track| track.label.clone())
                .unwrap_or_else(|| format!("Track {}", index + 1)),
            None => "Subtitles off".into(),
        }
    }

    pub fn load_external(&mut self, uri: &Url) {
        self.pipeline.set_property("suburi", uri.as_str());
        self.refresh_tracks();
        if let Some(last) = self.tracks.last() {
            self.set_current(Some(last.index));
        }
    }
}

fn video_appsink(pipeline: &gst::Pipeline) -> Result<gst_app::AppSink, String> {
    let video_sink: gst::Element = pipeline.property("video-sink");
    let pad = video_sink
        .pads()
        .first()
        .cloned()
        .ok_or_else(|| "video sink has no pads".to_string())?;
    let pad = pad
        .dynamic_cast::<gst::GhostPad>()
        .map_err(|_| "video sink pad is not a ghost pad".to_string())?;
    let bin = pad
        .parent_element()
        .ok_or_else(|| "video sink has no parent".to_string())?
        .downcast::<gst::Bin>()
        .map_err(|_| "video sink parent is not a bin".to_string())?;
    let sink = bin
        .by_name("gpui_video")
        .ok_or_else(|| "missing gpui_video appsink".to_string())?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| "gpui_video is not an appsink".to_string())?;
    Ok(sink)
}

fn set_play_flag_text(pipeline: &gst::Pipeline, enabled: bool) {
    let value = pipeline.property_value("flags");
    let Some(class) = FlagsClass::with_type(value.type_()) else {
        return;
    };
    let Some(builder) = class.builder_with_value(value) else {
        return;
    };
    let builder = if enabled {
        builder.set_by_nick("text")
    } else {
        builder.unset_by_nick("text")
    };
    if let Some(value) = builder.build() {
        pipeline.set_property("flags", value);
    }
}

fn set_overlay_visible(pipeline: &gst::Pipeline, visible: bool) {
    let mut iter = pipeline.iterate_recurse();
    loop {
        match iter.next() {
            Ok(Some(element)) => {
                let factory = element
                    .factory()
                    .map(|factory| factory.name().as_str().to_string())
                    .unwrap_or_default();
                match factory.as_str() {
                    "subtitleoverlay" => element.set_property("silent", !visible),
                    "assrender" => element.set_property("enable", visible),
                    _ => {}
                }
            }
            Ok(None) => break,
            Err(gst::IteratorError::Resync) => iter.resync(),
            Err(_) => break,
        }
    }
}

fn track_label(pipeline: &gst::Pipeline, index: i32) -> String {
    let tags = pipeline.emit_by_name::<Option<gst::TagList>>("get-text-tags", &[&index]);
    let mut parts = Vec::new();
    if let Some(tags) = tags {
        if let Some(lang) = tags.get::<gst::tags::LanguageCode>() {
            let code = lang.get();
            if !code.is_empty() {
                parts.push(language_name(code));
            }
        }
        if let Some(title) = tags.get::<gst::tags::Title>() {
            let title = title.get();
            if !title.is_empty() {
                parts.push(title.to_string());
            }
        }
    }
    if parts.is_empty() {
        format!("Track {}", index + 1)
    } else {
        parts.join(" · ")
    }
}

fn language_name(code: &str) -> String {
    match code {
        "en" | "eng" => "English".into(),
        "zh" | "chi" | "zho" | "zh-cn" | "zh-hans" => "中文".into(),
        "zh-tw" | "zh-hant" => "繁體中文".into(),
        "ja" | "jpn" => "日本語".into(),
        "ko" | "kor" => "한국어".into(),
        "fr" | "fra" | "fre" => "Français".into(),
        "de" | "deu" | "ger" => "Deutsch".into(),
        "es" | "spa" => "Español".into(),
        "pt" | "por" => "Português".into(),
        "ru" | "rus" => "Русский".into(),
        other => other.to_string(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn clean_subtitle_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                }
            }
            '<' => {
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                }
            }
            '\\' => match chars.peek() {
                Some('N' | 'n' | 'h') => {
                    chars.next();
                    out.push('\n');
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            other => out.push(other),
        }
    }
    out.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn next_subtitle_selection(tracks: &[SubtitleTrack], current: Option<i32>) -> Option<i32> {
    if tracks.is_empty() {
        return None;
    }
    match current {
        None => Some(tracks[0].index),
        Some(current) => match tracks.iter().position(|track| track.index == current) {
            Some(pos) if pos + 1 < tracks.len() => Some(tracks[pos + 1].index),
            _ => None,
        },
    }
}

fn escape_launch_uri(uri: &str) -> String {
    uri.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn sidecar_uri(path: &Path) -> Option<Url> {
    for ext in util::SUBTITLE_EXTENSIONS {
        let candidate = path.with_extension(ext);
        if candidate.is_file() {
            return Url::from_file_path(candidate).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ass_and_html_markup() {
        assert_eq!(
            clean_subtitle_text(r"{\i1}Hello\Nworld{\i0}"),
            "Hello\nworld"
        );
        assert_eq!(clean_subtitle_text("<i>Hi</i> there"), "Hi there");
    }

    #[test]
    fn subtitle_cycle_wraps_from_off_back_to_first_track() {
        let tracks = vec![
            SubtitleTrack {
                index: 0,
                label: "English".into(),
            },
            SubtitleTrack {
                index: 1,
                label: "中文".into(),
            },
        ];
        assert_eq!(next_subtitle_selection(&tracks, Some(0)), Some(1));
        assert_eq!(next_subtitle_selection(&tracks, Some(1)), None);
        assert_eq!(next_subtitle_selection(&tracks, None), Some(0));
        assert_eq!(next_subtitle_selection(&tracks, Some(99)), None);
        assert_eq!(next_subtitle_selection(&[], None), None);
    }
}
