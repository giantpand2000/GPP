use gpui_video_player::gst;
use gpui_video_player::{Error as VideoError, Video, VideoOptions};
use gst::prelude::*;
use gstreamer_app as gst_app;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use url::Url;

use crate::util;

#[derive(Clone, Debug)]
pub struct SubtitleTrack {
    pub index: i32,
    pub label: String,
}

#[derive(Clone, Debug)]
struct Cue {
    start: Duration,
    end: Duration,
    text: String,
}

pub struct SubtitleSession {
    pipeline: gst::Pipeline,
    text_sink: gst_app::AppSink,
    tracks: Vec<SubtitleTrack>,
    current: Option<i32>,
    cues: Mutex<VecDeque<Cue>>,
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

    let text_sink = make_text_sink()?;
    pipeline.set_property("text-sink", text_sink.upcast_ref::<gst::Element>());
    if let Some(sidecar) = sidecar {
        pipeline.set_property("suburi", sidecar.as_str());
    }

    let video_sink = video_appsink(&pipeline)?;
    let video = Video::from_gst_pipeline_with_options(pipeline.clone(), video_sink, None, options)
        .map_err(|err: VideoError| err.to_string())?;

    let mut subtitles = SubtitleSession {
        pipeline,
        text_sink,
        tracks: Vec::new(),
        current: None,
        cues: Mutex::new(VecDeque::new()),
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
        // Do not set current-text to -1. playbin then either reports n-text=0
        // or clamps back to the last stream, so cycling cannot leave Off.
        if let Some(index) = index {
            self.pipeline.set_property("current-text", index);
        }
        if let Ok(mut cues) = self.cues.lock() {
            cues.clear();
        }
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

    pub fn cue_at(&self, position: Duration) -> Option<String> {
        if self.current.is_none() {
            return None;
        }
        self.drain_samples();
        let Ok(mut cues) = self.cues.lock() else {
            return None;
        };
        while cues
            .front()
            .is_some_and(|cue| cue.end + Duration::from_millis(80) < position)
        {
            cues.pop_front();
        }
        cues.iter()
            .rev()
            .find(|cue| position >= cue.start && position <= cue.end)
            .map(|cue| cue.text.clone())
    }

    fn drain_samples(&self) {
        let Ok(mut cues) = self.cues.lock() else {
            return;
        };
        while let Some(sample) = self.text_sink.try_pull_sample(gst::ClockTime::ZERO) {
            if let Some(cue) = cue_from_sample(&sample) {
                cues.push_back(cue);
                while cues.len() > 24 {
                    cues.pop_front();
                }
            }
        }
    }
}

fn make_text_sink() -> Result<gst_app::AppSink, String> {
    let element = gst::ElementFactory::make("appsink")
        .name("gpp_text")
        .property("drop", true)
        .property("max-buffers", 32u32)
        .property("enable-last-sample", false)
        .property("sync", false)
        .property("emit-signals", false)
        .build()
        .map_err(|err| err.to_string())?;
    let sink = element
        .downcast::<gst_app::AppSink>()
        .map_err(|_| "failed to create subtitle appsink".to_string())?;
    sink.set_caps(Some(&gst::Caps::builder("text/x-raw").build()));
    Ok(sink)
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

fn cue_from_sample(sample: &gst::Sample) -> Option<Cue> {
    let buffer = sample.buffer()?;
    let pts = buffer.pts()?;
    let duration = buffer
        .duration()
        .unwrap_or(gst::ClockTime::from_mseconds(3500));
    let map = buffer.map_readable().ok()?;
    let raw = std::str::from_utf8(map.as_slice()).ok()?;
    let text = clean_subtitle_text(raw);
    if text.is_empty() {
        return None;
    }
    Some(Cue {
        start: Duration::from_nanos(pts.nseconds()),
        end: Duration::from_nanos(pts.nseconds().saturating_add(duration.nseconds())),
        text,
    })
}

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
