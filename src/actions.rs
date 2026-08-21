use gpui::actions;

actions!(
    gpp,
    [
        Quit,
        OpenFile,
        PlayPause,
        SeekBack,
        SeekForward,
        SeekBackLarge,
        SeekForwardLarge,
        VolumeUp,
        VolumeDown,
        ToggleMute,
        ToggleLoop,
        ToggleFullscreen,
        ExitFullscreen,
        ToggleSettings,
        ShowAbout,
        CycleSpeed,
        CycleSubtitles,
        ToggleDanmaku,
        NextTrack,
        PrevTrack,
        Restart,
    ]
);
