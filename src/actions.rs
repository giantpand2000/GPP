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
        CycleSpeed,
        NextTrack,
        PrevTrack,
        Restart,
    ]
);
