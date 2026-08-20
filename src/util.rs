use std::path::{Path, PathBuf};
use std::time::Duration;
use url::Url;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "mov", "avi", "wmv", "flv", "ts", "mts", "m2ts", "mpeg", "mpg",
    "ogv", "3gp", "asf", "vob", "mxf", "rmvb", "m3u8", "mpd",
];

pub const SPEED_PRESETS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

#[derive(Clone, Debug)]
pub enum MediaSource {
    File(PathBuf),
    Url(Url),
}

impl MediaSource {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("empty path".into());
        }
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("file://")
            || trimmed.starts_with("rtsp://")
        {
            Url::parse(trimmed)
                .map(Self::Url)
                .map_err(|err| format!("invalid URL: {err}"))
        } else {
            Ok(Self::File(PathBuf::from(trimmed)))
        }
    }

    pub fn from_path(path: PathBuf) -> Self {
        Self::File(path)
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::File(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            Self::Url(url) => url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .filter(|segment| !segment.is_empty())
                .map(|segment| percent_decode(segment))
                .unwrap_or_else(|| url.to_string()),
        }
    }

    pub fn to_url(&self) -> Result<Url, String> {
        match self {
            Self::File(path) => {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                Url::from_file_path(&canonical)
                    .map_err(|_| format!("invalid file path: {}", canonical.display()))
            }
            Self::Url(url) => Ok(url.clone()),
        }
    }
}

pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|candidate| ext.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

pub fn collect_media(paths: impl IntoIterator<Item = PathBuf>) -> Vec<MediaSource> {
    let mut media = Vec::new();
    for path in paths {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut children: Vec<_> = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|child| child.is_file() && is_video_path(child))
                    .collect();
                children.sort();
                media.extend(children.into_iter().map(MediaSource::from_path));
            }
        } else if is_video_path(&path) || path.is_file() {
            media.push(MediaSource::from_path(path));
        }
    }
    media
}

pub fn format_duration(duration: Duration) -> String {
    format_player_time(duration)
}

pub fn format_player_time(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn next_speed(current: f64) -> f64 {
    SPEED_PRESETS
        .iter()
        .copied()
        .find(|speed| *speed > current + 0.01)
        .unwrap_or(SPEED_PRESETS[0])
}

pub fn format_speed(speed: f64) -> String {
    if (speed.fract()).abs() < 0.01 {
        format!("{}x", speed.round() as i32)
    } else {
        let text = format!("{speed:.2}");
        format!("{}x", text.trim_end_matches('0').trim_end_matches('.'))
    }
}

fn percent_decode(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(value) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                output.push(value as char);
                i += 3;
                continue;
            }
        }
        output.push(bytes[i] as char);
        i += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_minutes_and_hours() {
        assert_eq!(format_duration(Duration::from_secs(5)), "0:05");
        assert_eq!(format_duration(Duration::from_secs(75)), "1:15");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn cycles_speed_presets() {
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(2.0), 0.5);
    }
}
