use crate::actions::*;
use crate::icon::{self, Icon};
use crate::settings::{self, Settings};
use crate::subtitles::{self, SubtitleSession};
use crate::theme;
use crate::util::{
    self, MediaSource, collect_media, format_duration, format_speed, is_subtitle_path,
    is_video_path, next_speed,
};
use gpui::{
    App, Bounds, ClickEvent, Context, CursorStyle, ExternalPaths, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, SharedString, Timer, Window, canvas, div, prelude::*, px,
};
use gpui_video_player::{Video, VideoOptions, video as video_el};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use url::Url;

const SEEK_STEP: Duration = Duration::from_secs(5);
const SEEK_STEP_LARGE: Duration = Duration::from_secs(15);
const HIDE_CONTROLS_AFTER: Duration = Duration::from_millis(2200);

#[derive(Clone, Copy, PartialEq, Eq)]
enum DragTarget {
    Seek,
    Volume,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Empty,
    Loading,
    Ready,
}

pub struct Player {
    focus_handle: FocusHandle,
    playlist: Vec<MediaSource>,
    index: usize,
    video: Option<Video>,
    status: Status,
    error: Option<String>,
    looping: bool,
    muted: bool,
    volume: f64,
    speed: f64,
    controls_visible: bool,
    last_interaction: Instant,
    stage_bounds: Bounds<Pixels>,
    seek_bounds: Bounds<Pixels>,
    volume_bounds: Bounds<Pixels>,
    dragging: Option<DragTarget>,
    scrub: Option<f32>,
    load_generation: u64,
    dialog_open: bool,
    subtitles: Option<SubtitleSession>,
    subtitle_toast: Option<(SharedString, Instant)>,
    settings: Settings,
    settings_open: bool,
}

impl Player {
    pub fn new(initial: Option<MediaSource>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let settings = Settings::load();

        let mut player = Self {
            focus_handle,
            playlist: Vec::new(),
            index: 0,
            video: None,
            status: Status::Empty,
            error: None,
            looping: settings.loop_playback,
            muted: settings.muted,
            volume: settings.volume,
            speed: settings.speed,
            controls_visible: true,
            last_interaction: Instant::now(),
            stage_bounds: Bounds::default(),
            seek_bounds: Bounds::default(),
            volume_bounds: Bounds::default(),
            dragging: None,
            scrub: None,
            load_generation: 0,
            dialog_open: false,
            subtitles: None,
            subtitle_toast: None,
            settings,
            settings_open: false,
        };

        if let Some(source) = initial {
            player.playlist = vec![source];
            player.open_current(cx);
        }

        player
    }

    fn bump_interaction(&mut self) {
        self.last_interaction = Instant::now();
        self.controls_visible = true;
    }

    fn current_title(&self) -> String {
        self.playlist
            .get(self.index)
            .map(|source| source.display_name())
            .unwrap_or_else(|| "GPP".into())
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open {
            return;
        }
        self.bump_interaction();
        self.dialog_open = true;

        // NSOpenPanel.runModal() re-enters the Cocoa run loop while GPUI still
        // holds the window RefCell from this click/action. Use the async sheet
        // API after yielding so the current event has finished.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Open video")
            .add_filter("Video", util::VIDEO_EXTENSIONS)
            .add_filter("All files", &["*"])
            .set_parent(&*window);

        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(16)).await;
            let picked = dialog.pick_file().await;
            this.update(cx, |this, cx| {
                this.dialog_open = false;
                if let Some(file) = picked {
                    this.load_sources(
                        vec![MediaSource::from_path(file.path().to_path_buf())],
                        0,
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
    }

    fn load_sources(&mut self, sources: Vec<MediaSource>, start: usize, cx: &mut Context<Self>) {
        if sources.is_empty() {
            self.error = Some("No playable files found".into());
            self.status = Status::Empty;
            self.video = None;
            self.subtitles = None;
            cx.notify();
            return;
        }
        self.playlist = sources;
        self.index = start.min(self.playlist.len() - 1);
        self.open_current(cx);
    }

    fn open_current(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.playlist.get(self.index).cloned() else {
            return;
        };
        let uri = match source.to_url() {
            Ok(uri) => uri,
            Err(err) => {
                self.error = Some(err);
                self.status = Status::Empty;
                self.video = None;
                cx.notify();
                return;
            }
        };

        self.status = Status::Loading;
        self.error = None;
        self.video = None;
        self.subtitles = None;
        self.subtitle_toast = None;
        self.scrub = None;
        self.load_generation += 1;
        let generation = self.load_generation;
        let looping = self.looping;
        let speed = self.speed;
        let volume = self.volume;
        let muted = self.muted;
        let sidecar = match &source {
            MediaSource::File(path) => subtitles::sidecar_uri(path),
            MediaSource::Url(_) => None,
        };

        let task = cx.background_spawn(async move {
            subtitles::open(
                &uri,
                sidecar.as_ref(),
                VideoOptions {
                    frame_buffer_capacity: Some(4),
                    looping: Some(looping),
                    speed: Some(if (speed - 1.0).abs() < f64::EPSILON {
                        1.0
                    } else {
                        speed
                    }),
                    ..VideoOptions::default()
                },
            )
        });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if this.load_generation != generation {
                    return;
                }
                match result {
                    Ok(mut opened) => {
                        opened.video.set_volume(volume);
                        opened.video.set_muted(muted);
                        if (speed - 1.0).abs() > f64::EPSILON {
                            let _ = opened.video.set_speed(speed);
                        }
                        this.sync_display_size(&opened.video);
                        if !this.settings.autoplay {
                            opened.video.set_paused(true);
                        }
                        if !this.settings.subtitle_enabled {
                            opened.subtitles.set_current(None);
                        }
                        this.video = Some(opened.video);
                        this.subtitles = Some(opened.subtitles);
                        this.status = Status::Ready;
                        this.error = None;
                    }
                    Err(err) => {
                        this.video = None;
                        this.subtitles = None;
                        this.status = Status::Empty;
                        this.error = Some(format!("Failed to open video: {err}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();

        cx.notify();
    }

    fn sync_display_size(&self, video: &Video) {
        let width = f32::from(self.stage_bounds.size.width).max(1.0) as u32;
        let height = f32::from(self.stage_bounds.size.height).max(1.0) as u32;
        if width > 1 && height > 1 {
            video.set_display_size(Some(width), Some(height));
        }
    }

    fn toggle_play(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        if let Some(video) = self.video.clone() {
            if video.eos() {
                let _ = video.restart_stream();
            } else {
                video.set_paused(!video.paused());
            }
        } else if !self.playlist.is_empty() {
            self.open_current(cx);
        }
        cx.notify();
    }

    fn seek_by(&mut self, delta: Duration, backward: bool, cx: &mut Context<Self>) {
        self.bump_interaction();
        let Some(video) = self.video.clone() else {
            return;
        };
        let position = video.position();
        let duration = video.duration();
        let target = if backward {
            position.saturating_sub(delta)
        } else {
            (position + delta).min(duration)
        };
        let _ = video.seek(target, false);
        cx.notify();
    }

    fn seek_ratio(&mut self, ratio: f32, accurate: bool, cx: &mut Context<Self>) {
        let Some(video) = self.video.clone() else {
            return;
        };
        let duration = video.duration();
        if duration.is_zero() {
            return;
        }
        let nanos = (duration.as_secs_f64() * ratio.clamp(0.0, 1.0) as f64) as u64;
        let _ = video.seek(Duration::from_nanos(nanos), accurate);
        cx.notify();
    }

    fn adjust_volume(&mut self, delta: f64, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.set_volume((self.volume + delta).clamp(0.0, 1.0), cx);
    }

    fn set_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        self.volume = volume.clamp(0.0, 1.0);
        if self.volume > 0.0 {
            self.muted = false;
        }
        if let Some(video) = &self.video {
            video.set_volume(self.volume);
            video.set_muted(self.muted);
        }
        self.settings.volume = self.volume;
        self.settings.save();
        cx.notify();
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.muted = !self.muted;
        if let Some(video) = &self.video {
            video.set_muted(self.muted);
        }
        self.settings.muted = self.muted;
        self.settings.save();
        cx.notify();
    }

    fn toggle_loop(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.looping = !self.looping;
        if let Some(video) = &self.video {
            video.set_looping(self.looping);
        }
        self.settings.loop_playback = self.looping;
        self.settings.save();
        cx.notify();
    }

    fn cycle_speed(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.speed = next_speed(self.speed);
        if let Some(video) = &self.video {
            let _ = video.set_speed(self.speed);
        }
        self.settings.speed = self.speed;
        self.settings.save();
        cx.notify();
    }

    fn set_speed(&mut self, speed: f64, cx: &mut Context<Self>) {
        self.speed = speed;
        if let Some(video) = &self.video {
            let _ = video.set_speed(self.speed);
        }
        self.settings.speed = self.speed;
        self.settings.save();
        cx.notify();
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.controls_visible = true;
        }
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_open {
            self.settings_open = false;
            cx.notify();
        }
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        if let Some(video) = self.video.clone() {
            let _ = video.restart_stream();
        }
        cx.notify();
    }

    fn next_track(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        if self.playlist.len() < 2 {
            if self.looping && !self.playlist.is_empty() {
                self.open_current(cx);
            }
            return;
        }
        self.index = (self.index + 1) % self.playlist.len();
        self.open_current(cx);
    }

    fn prev_track(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        if self.playlist.len() < 2 {
            self.restart(cx);
            return;
        }
        if self.index == 0 {
            self.index = self.playlist.len() - 1;
        } else {
            self.index -= 1;
        }
        self.open_current(cx);
    }

    fn handle_drop(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        self.bump_interaction();
        let subtitle = paths
            .paths()
            .iter()
            .find(|path| is_subtitle_path(path))
            .cloned();
        let sources = collect_media(paths.paths().iter().cloned());
        if sources.is_empty() {
            if let Some(path) = subtitle {
                self.load_external_subtitle(path, cx);
            }
            return;
        }
        self.load_sources(sources, 0, cx);
    }

    fn load_external_subtitle(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(session) = self.subtitles.as_mut() else {
            self.error = Some("Open a video before dropping a subtitle file".into());
            cx.notify();
            return;
        };
        match Url::from_file_path(path.canonicalize().unwrap_or(path)) {
            Ok(uri) => {
                session.load_external(&uri);
                let label = session.current_label().unwrap_or("Subtitles").to_string();
                self.subtitle_toast = Some((label.into(), Instant::now()));
            }
            Err(()) => {
                self.error = Some("Invalid subtitle path".into());
            }
        }
        cx.notify();
    }

    fn cycle_subtitles(&mut self, cx: &mut Context<Self>) {
        self.bump_interaction();
        let Some(session) = self.subtitles.as_mut() else {
            return;
        };
        let label = session.cycle();
        self.subtitle_toast = Some((label.into(), Instant::now()));
        cx.notify();
    }

    fn ratio_at(bounds: Bounds<Pixels>, x: Pixels) -> f32 {
        let width = f32::from(bounds.size.width).max(1.0);
        ((f32::from(x) - f32::from(bounds.origin.x)) / width).clamp(0.0, 1.0)
    }

    fn on_pointer_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        self.bump_interaction();
        match self.dragging {
            Some(DragTarget::Seek) => {
                self.scrub = Some(Self::ratio_at(self.seek_bounds, event.position.x));
                cx.notify();
            }
            Some(DragTarget::Volume) => {
                self.set_volume(
                    Self::ratio_at(self.volume_bounds, event.position.x) as f64,
                    cx,
                );
            }
            None => cx.notify(),
        }
    }

    fn finish_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        match self.dragging.take() {
            Some(DragTarget::Seek) => {
                let ratio = self
                    .scrub
                    .unwrap_or_else(|| Self::ratio_at(self.seek_bounds, position.x));
                self.scrub = None;
                self.seek_ratio(ratio, true, cx);
            }
            Some(DragTarget::Volume) => {
                self.set_volume(Self::ratio_at(self.volume_bounds, position.x) as f64, cx);
            }
            None => {}
        }
    }

    fn begin_seek(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.dragging = Some(DragTarget::Seek);
        let ratio = Self::ratio_at(self.seek_bounds, event.position.x);
        self.scrub = Some(ratio);
        self.seek_ratio(ratio, true, cx);
    }

    fn begin_volume(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.bump_interaction();
        self.dragging = Some(DragTarget::Volume);
        self.set_volume(
            Self::ratio_at(self.volume_bounds, event.position.x) as f64,
            cx,
        );
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(16.));
        let y: f32 = delta.y.into();
        if y.abs() < f32::EPSILON {
            return;
        }
        let step = if y > 0.0 { 0.05 } else { -0.05 };
        self.adjust_volume(step, cx);
    }

    fn maybe_advance_on_eos(&mut self, cx: &mut Context<Self>) {
        let Some(video) = self.video.clone() else {
            return;
        };
        if !video.eos() || self.looping || !self.settings.autoplay {
            return;
        }
        if self.playlist.len() > 1 && self.index + 1 < self.playlist.len() {
            self.index += 1;
            self.open_current(cx);
        }
    }

    fn playback_progress(&self) -> f32 {
        if let Some(ratio) = self.scrub {
            return ratio;
        }
        let Some(video) = &self.video else {
            return 0.0;
        };
        let duration = video.duration().as_secs_f64();
        if duration <= 0.0 {
            0.0
        } else {
            (video.position().as_secs_f64() / duration).clamp(0.0, 1.0) as f32
        }
    }

    fn displayed_position(&self) -> Duration {
        if let (Some(ratio), Some(video)) = (self.scrub, self.video.as_ref()) {
            Duration::from_secs_f64(video.duration().as_secs_f64() * ratio as f64)
        } else {
            self.video
                .as_ref()
                .map(|video| video.position())
                .unwrap_or(Duration::ZERO)
        }
    }
}

impl Focusable for Player {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Player {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.maybe_advance_on_eos(cx);

        let playing = self
            .video
            .as_ref()
            .map(|video| !video.paused() && !video.eos())
            .unwrap_or(false);

        if playing {
            window.request_animation_frame();
            if self.settings.auto_hide_controls
                && !self.settings_open
                && self.dragging.is_none()
                && self.last_interaction.elapsed() > HIDE_CONTROLS_AFTER
            {
                self.controls_visible = false;
            }
        } else {
            self.controls_visible = true;
        }

        window.set_window_title(&format!("{} — GPP", self.current_title()));

        if self
            .subtitle_toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(2))
        {
            self.subtitle_toast = None;
        }
        if let Some(session) = self.subtitles.as_mut()
            && session.tracks().is_empty()
        {
            session.refresh_tracks();
        }

        let show_chrome = self.controls_visible || self.status != Status::Ready || !playing;
        let entity = cx.entity();

        div()
            .id("player")
            .key_context("Player")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_weight(FontWeight::MEDIUM)
            .on_action(cx.listener(|this, _: &OpenFile, window, cx| this.open_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &PlayPause, _, cx| this.toggle_play(cx)))
            .on_action(cx.listener(|this, _: &SeekBack, _, cx| this.seek_by(SEEK_STEP, true, cx)))
            .on_action(
                cx.listener(|this, _: &SeekForward, _, cx| this.seek_by(SEEK_STEP, false, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SeekBackLarge, _, cx| {
                    this.seek_by(SEEK_STEP_LARGE, true, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SeekForwardLarge, _, cx| {
                this.seek_by(SEEK_STEP_LARGE, false, cx)
            }))
            .on_action(cx.listener(|this, _: &VolumeUp, _, cx| this.adjust_volume(0.05, cx)))
            .on_action(cx.listener(|this, _: &VolumeDown, _, cx| this.adjust_volume(-0.05, cx)))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| this.toggle_mute(cx)))
            .on_action(cx.listener(|this, _: &ToggleLoop, _, cx| this.toggle_loop(cx)))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                this.bump_interaction();
                window.toggle_fullscreen();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ExitFullscreen, window, cx| {
                if this.settings_open {
                    this.close_settings(cx);
                    return;
                }
                if window.is_fullscreen() {
                    this.bump_interaction();
                    window.toggle_fullscreen();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSettings, _, cx| this.toggle_settings(cx)))
            .on_action(cx.listener(|this, _: &CycleSpeed, _, cx| this.cycle_speed(cx)))
            .on_action(cx.listener(|this, _: &CycleSubtitles, _, cx| this.cycle_subtitles(cx)))
            .on_action(cx.listener(|this, _: &NextTrack, _, cx| this.next_track(cx)))
            .on_action(cx.listener(|this, _: &PrevTrack, _, cx| this.prev_track(cx)))
            .on_action(cx.listener(|this, _: &Restart, _, cx| this.restart(cx)))
            .on_mouse_move(cx.listener(|this, event, _, cx| this.on_pointer_move(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.finish_drag(event.position, cx)
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    this.finish_drag(event.position, cx)
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event, _, cx| this.on_scroll(event, cx)))
            .can_drop(|value, _, _| {
                value
                    .downcast_ref::<ExternalPaths>()
                    .map(|paths| {
                        paths
                            .paths()
                            .iter()
                            .any(|path| path.is_dir() || is_video_path(path) || path.is_file())
                    })
                    .unwrap_or(false)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.handle_drop(paths, cx)))
            .drag_over::<ExternalPaths>(|style, _, _, _| {
                style.border_2().border_color(theme::progress())
            })
            .child(self.render_stage(entity.clone(), cx))
            .child(self.render_subtitle_overlay(show_chrome))
            .when(show_chrome, |this| {
                this.child(self.render_top_bar(cx))
                    .child(self.render_controls(entity, window, cx))
            })
            .when(self.settings_open, |this| {
                this.child(self.render_settings_overlay(cx))
            })
    }
}

impl Player {
    fn render_stage(&self, entity: gpui::Entity<Self>, cx: &mut Context<Self>) -> impl IntoElement {
        let video = self.video.clone();
        let status = self.status;
        let error = self.error.clone();
        let title = self.current_title();

        div()
            .id("stage")
            .relative()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(theme::bg())
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                if event.click_count() >= 2 {
                    this.bump_interaction();
                    window.toggle_fullscreen();
                    cx.notify();
                } else if event.standard_click() && this.video.is_some() {
                    this.toggle_play(cx);
                }
            }))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, cx| {
                            let changed = this.stage_bounds.size != bounds.size;
                            this.stage_bounds = bounds;
                            if changed {
                                if let Some(video) = this.video.clone() {
                                    this.sync_display_size(&video);
                                }
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when_some(video, |this, handle| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(video_el(handle).id("frame").buffer_capacity(4)),
                )
            })
            .when(status == Status::Empty && error.is_none(), |this| {
                this.child(empty_state(title, cx))
            })
            .when(status == Status::Loading, |this| {
                this.child(status_overlay("Opening…", None))
            })
            .when_some(error, |this, error| {
                this.child(status_overlay("Couldn't play this file", Some(error)))
            })
    }

    fn render_subtitle_overlay(&self, chrome_visible: bool) -> impl IntoElement {
        let cue = self
            .subtitles
            .as_ref()
            .and_then(|session| session.cue_at(self.displayed_position()));
        let toast = self.subtitle_toast.as_ref().map(|(label, _)| label.clone());

        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(if chrome_visible { px(76.) } else { px(28.) })
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .when_some(toast, |this, label| {
                this.child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(theme::overlay())
                        .text_xs()
                        .text_color(theme::muted())
                        .child(label),
                )
            })
            .when_some(cue, |this, text| {
                this.child(
                    div()
                        .max_w(px(860.))
                        .px_3()
                        .py_1()
                        .rounded_sm()
                        .when(self.settings.subtitle_background, |this| {
                            this.bg(theme::overlay())
                        })
                        .text_color(theme::white())
                        .text_size(px(self.settings.subtitle_size))
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_1()
                                .children(text.lines().map(|line| line.to_string())),
                        ),
                )
            })
    }

    fn render_settings_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let autoplay = self.settings.autoplay;
        let looping = self.settings.loop_playback;
        let auto_hide = self.settings.auto_hide_controls;
        let speed = self.settings.speed;
        let volume = self.settings.volume;
        let subs_on = self.settings.subtitle_enabled;
        let subs_bg = self.settings.subtitle_background;
        let subs_size = self.settings.subtitle_size;

        div()
            .id("settings-overlay")
            .absolute()
            .size_full()
            .top_0()
            .left_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::settings_veil())
            .on_click(cx.listener(|this, _, _, cx| this.close_settings(cx)))
            .child(
                div()
                    .id("settings-panel")
                    .w(px(420.))
                    .max_h(px(560.))
                    .rounded_xl()
                    .bg(theme::settings_panel())
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px_5()
                            .h(px(56.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme::white())
                                    .child("Settings"),
                            )
                            .child(icon::icon_button(
                                "settings-close",
                                Icon::Close,
                                false,
                                false,
                                cx.listener(|this, _, _, cx| this.close_settings(cx)),
                            )),
                    )
                    .child(
                        div()
                            .id("settings-scroll")
                            .flex_1()
                            .px_5()
                            .pb_5()
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .child(settings::section_label("PLAYBACK"))
                            .child(settings::setting_row(
                                "Autoplay",
                                settings::toggle(
                                    "set-autoplay",
                                    autoplay,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings.autoplay = !this.settings.autoplay;
                                        this.settings.save();
                                        cx.notify();
                                    }),
                                ),
                            ))
                            .child(settings::setting_row(
                                "Loop by default",
                                settings::toggle(
                                    "set-loop",
                                    looping,
                                    cx.listener(|this, _, _, cx| this.toggle_loop(cx)),
                                ),
                            ))
                            .child(settings::setting_row(
                                "Auto-hide controls",
                                settings::toggle(
                                    "set-autohide",
                                    auto_hide,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings.auto_hide_controls =
                                            !this.settings.auto_hide_controls;
                                        this.settings.save();
                                        cx.notify();
                                    }),
                                ),
                            ))
                            .child(
                                div()
                                    .pt_1()
                                    .pb_1()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child("Default speed"),
                            )
                            .child(div().flex().items_center().gap_1().children(
                                util::SPEED_PRESETS.iter().copied().map(|preset| {
                                    settings::choice_chip(
                                        format!("speed-{preset}"),
                                        format_speed(preset),
                                        (preset - speed).abs() < 0.01,
                                        cx.listener(move |this, _, _, cx| {
                                            this.set_speed(preset, cx)
                                        }),
                                    )
                                }),
                            ))
                            .child(settings::section_label("AUDIO"))
                            .child(
                                div()
                                    .pt_1()
                                    .pb_1()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child("Default volume"),
                            )
                            .child(div().flex().items_center().gap_1().children(
                                settings::VOLUME_PRESETS.iter().copied().map(|preset| {
                                    settings::choice_chip(
                                        format!("vol-{preset}"),
                                        format!("{}%", (preset * 100.0).round() as i32),
                                        (preset - volume).abs() < 0.01,
                                        cx.listener(move |this, _, _, cx| {
                                            this.muted = false;
                                            this.set_volume(preset, cx);
                                        }),
                                    )
                                }),
                            ))
                            .child(settings::section_label("SUBTITLES"))
                            .child(settings::setting_row(
                                "Show by default",
                                settings::toggle(
                                    "set-subs",
                                    subs_on,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings.subtitle_enabled =
                                            !this.settings.subtitle_enabled;
                                        this.settings.save();
                                        if let Some(session) = this.subtitles.as_mut() {
                                            session.refresh_tracks();
                                            if this.settings.subtitle_enabled {
                                                if session.current().is_none() {
                                                    if let Some(track) = session.tracks().first() {
                                                        let index = track.index;
                                                        session.set_current(Some(index));
                                                    }
                                                }
                                            } else {
                                                session.set_current(None);
                                            }
                                        }
                                        cx.notify();
                                    }),
                                ),
                            ))
                            .child(settings::setting_row(
                                "Background",
                                settings::toggle(
                                    "set-subs-bg",
                                    subs_bg,
                                    cx.listener(|this, _, _, cx| {
                                        this.settings.subtitle_background =
                                            !this.settings.subtitle_background;
                                        this.settings.save();
                                        cx.notify();
                                    }),
                                ),
                            ))
                            .child(settings::setting_row(
                                "Default size",
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(settings::choice_chip(
                                        "sub-smaller".into(),
                                        "A-",
                                        false,
                                        cx.listener(|this, _, _, cx| {
                                            this.settings.cycle_subtitle_size(false);
                                            this.settings.save();
                                            cx.notify();
                                        }),
                                    ))
                                    .child(
                                        div()
                                            .w(px(40.))
                                            .text_xs()
                                            .text_color(theme::white())
                                            .child(format!("{}px", subs_size as i32)),
                                    )
                                    .child(settings::choice_chip(
                                        "sub-larger".into(),
                                        "A+",
                                        false,
                                        cx.listener(|this, _, _, cx| {
                                            this.settings.cycle_subtitle_size(true);
                                            this.settings.save();
                                            cx.notify();
                                        }),
                                    )),
                            )),
                    ),
            )
    }

    fn render_top_bar(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let playlist = if self.playlist.len() > 1 {
            Some(format!("{}/{}", self.index + 1, self.playlist.len()))
        } else {
            None
        };

        div()
            .id("top-bar")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .px_4()
            .pt_3()
            .pb_8()
            .bg(theme::top_gradient())
            .block_mouse_except_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::white())
                            .child(self.current_title()),
                    )
                    .when_some(playlist, |this, label| {
                        this.child(div().text_xs().text_color(theme::muted()).child(label))
                    }),
            )
    }

    fn render_controls(
        &self,
        entity: gpui::Entity<Self>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let paused = self
            .video
            .as_ref()
            .map(|video| video.paused() || video.eos())
            .unwrap_or(true);
        let progress = self.playback_progress();
        let position = format_duration(self.displayed_position());
        let duration = format_duration(
            self.video
                .as_ref()
                .map(|video| video.duration())
                .unwrap_or(Duration::ZERO),
        );
        let volume_ratio = if self.muted { 0.0 } else { self.volume as f32 };
        let volume_icon = if self.muted || self.volume == 0.0 {
            Icon::VolumeOff
        } else if self.volume < 0.5 {
            Icon::VolumeDown
        } else {
            Icon::VolumeUp
        };
        let play_icon = if paused { Icon::Play } else { Icon::Pause };
        let speed_label = format_speed(self.speed);
        let can_playlist = self.playlist.len() > 1;
        let has_video = self.video.is_some();
        let fullscreen = window.is_fullscreen();
        let volume_open = self.dragging == Some(DragTarget::Volume);

        div()
            .id("controls")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .pt_8()
            .pb_1()
            .bg(theme::bottom_gradient())
            .block_mouse_except_scroll()
            .flex()
            .flex_col()
            .child(self.render_seek_bar(progress, entity.clone(), cx))
            .child(
                div()
                    .id("control-bar")
                    .px_2()
                    .h(px(48.))
                    .flex()
                    .items_center()
                    .block_mouse_except_scroll()
                    .child(icon::icon_button(
                        "play",
                        play_icon,
                        false,
                        false,
                        cx.listener(|this, _, _, cx| this.toggle_play(cx)),
                    ))
                    .child(icon::skip_button(
                        "back",
                        Icon::Replay,
                        "5",
                        !has_video,
                        cx.listener(|this, _, _, cx| this.seek_by(SEEK_STEP, true, cx)),
                    ))
                    .child(icon::skip_button(
                        "forward",
                        Icon::Forward,
                        "5",
                        !has_video,
                        cx.listener(|this, _, _, cx| this.seek_by(SEEK_STEP, false, cx)),
                    ))
                    .when(can_playlist, |this| {
                        this.child(icon::icon_button(
                            "next",
                            Icon::SkipNext,
                            false,
                            false,
                            cx.listener(|this, _, _, cx| this.next_track(cx)),
                        ))
                    })
                    .child(self.render_volume_cluster(
                        volume_icon,
                        volume_ratio,
                        volume_open,
                        entity,
                        cx,
                    ))
                    .child(
                        div()
                            .ml_1()
                            .flex()
                            .items_center()
                            .text_xs()
                            .text_color(theme::white())
                            .child(position)
                            .child(div().px_1().text_color(theme::muted()).child(" / "))
                            .child(div().text_color(theme::muted()).child(duration)),
                    )
                    .child(div().flex_1())
                    .child(icon::text_button(
                        "speed",
                        speed_label,
                        false,
                        !has_video,
                        cx.listener(|this, _, _, cx| this.cycle_speed(cx)),
                    ))
                    .child(icon::icon_button(
                        "loop",
                        Icon::Repeat,
                        self.looping,
                        false,
                        cx.listener(|this, _, _, cx| this.toggle_loop(cx)),
                    ))
                    .child(icon::icon_button(
                        "captions",
                        Icon::Captions,
                        self.subtitles
                            .as_ref()
                            .and_then(SubtitleSession::current)
                            .is_some(),
                        self.subtitles
                            .as_ref()
                            .map(|session| session.tracks().is_empty())
                            .unwrap_or(true),
                        cx.listener(|this, _, _, cx| this.cycle_subtitles(cx)),
                    ))
                    .child(icon::icon_button(
                        "settings",
                        Icon::Settings,
                        self.settings_open,
                        false,
                        cx.listener(|this, _, _, cx| this.toggle_settings(cx)),
                    ))
                    .child(icon::icon_button(
                        "open",
                        Icon::Folder,
                        false,
                        false,
                        cx.listener(|this, _, window, cx| this.open_dialog(window, cx)),
                    ))
                    .child(icon::icon_button(
                        "full",
                        if fullscreen {
                            Icon::FullscreenExit
                        } else {
                            Icon::Fullscreen
                        },
                        false,
                        false,
                        cx.listener(|this, _, window, cx| {
                            this.bump_interaction();
                            window.toggle_fullscreen();
                            cx.notify();
                        }),
                    )),
            )
    }

    fn render_seek_bar(
        &self,
        progress: f32,
        entity: gpui::Entity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let scrubbing = self.dragging == Some(DragTarget::Seek);
        let knob_size = px(13.);

        div()
            .id("seek-bar")
            .group("seek")
            .relative()
            .w_full()
            .h(px(16.))
            .px_3()
            .flex()
            .items_center()
            .cursor(CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.begin_seek(event, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _cx| {
                            this.seek_bounds = bounds;
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(3.))
                    .rounded_full()
                    .bg(theme::progress_track())
                    .group_hover("seek", |style| style.h(px(5.)))
                    .when(scrubbing, |this| this.h(px(5.)))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(progress))
                            .rounded_full()
                            .bg(theme::progress()),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(-5.))
                            .flex()
                            .w_full()
                            .child(div().w(gpui::relative(progress)))
                            .child(
                                div()
                                    .ml(px(-6.5))
                                    .size(knob_size)
                                    .rounded_full()
                                    .bg(theme::progress())
                                    .when(!scrubbing, |this| {
                                        this.opacity(0.)
                                            .group_hover("seek", |style| style.opacity(1.))
                                    }),
                            ),
                    ),
            )
    }

    fn render_volume_cluster(
        &self,
        volume_icon: Icon,
        ratio: f32,
        expanded: bool,
        entity: gpui::Entity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("volume-cluster")
            .group("volume")
            .h(px(40.))
            .flex()
            .items_center()
            .flex_none()
            .child(icon::icon_button(
                "mute",
                volume_icon,
                false,
                false,
                cx.listener(|this, _, _, cx| this.toggle_mute(cx)),
            ))
            .child(
                div()
                    .id("volume-slider-wrap")
                    .overflow_hidden()
                    .h_full()
                    .flex()
                    .items_center()
                    .w(if expanded { px(72.) } else { px(0.) })
                    .group_hover("volume", |style| style.w(px(72.)))
                    .child(self.render_volume_bar(ratio, entity, cx)),
            )
    }

    fn render_volume_bar(
        &self,
        ratio: f32,
        entity: gpui::Entity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("volume-bar")
            .relative()
            .w(px(64.))
            .h(px(40.))
            .ml_1()
            .flex()
            .items_center()
            .cursor(CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.begin_volume(event, cx)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _cx| {
                            this.volume_bounds = bounds;
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .w_full()
                    .h(px(3.))
                    .rounded_full()
                    .bg(theme::progress_track())
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(ratio))
                            .rounded_full()
                            .bg(theme::white()),
                    ),
            )
    }
}

fn empty_state(title: String, cx: &mut Context<Player>) -> impl IntoElement {
    div()
        .absolute()
        .size_full()
        .top_0()
        .left_0()
        .occlude()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .id("empty-play")
                .size(px(68.))
                .rounded_full()
                .bg(theme::progress())
                .flex()
                .items_center()
                .justify_center()
                .pl(px(4.))
                .cursor(CursorStyle::PointingHand)
                .hover(|style| style.opacity(0.9))
                .on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx)))
                .child(icon::icon(Icon::Play, 36.)),
        )
        .child(div().text_color(theme::white()).child(if title == "GPP" {
            SharedString::from("Drop a video here")
        } else {
            title.into()
        }))
        .child(
            div()
                .text_sm()
                .text_color(theme::muted())
                .child("or click to open a file"),
        )
}

fn status_overlay(title: &str, detail: Option<String>) -> impl IntoElement {
    div()
        .absolute()
        .size_full()
        .top_0()
        .left_0()
        .occlude()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .bg(theme::overlay())
        .child(div().text_color(theme::text()).child(title.to_string()))
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .max_w(px(520.))
                    .px_6()
                    .text_sm()
                    .text_color(theme::danger())
                    .child(detail),
            )
        })
}
