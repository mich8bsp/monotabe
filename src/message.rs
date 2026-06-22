use crate::model::song::{Instrument, InstrumentFilter, Song};

#[derive(Debug, Clone)]
pub enum Message {
    // Library
    SongsLoaded(Vec<Song>),
    SongSelected(String),
    FilterChanged(InstrumentFilter),
    SearchChanged(String),

    // CRUD triggers
    NewSong,
    EditSong(String),
    DeleteSong(String),
    ConfirmDeleteSong(String),
    CancelDelete,

    // Form field changes
    FormTitleChanged(String),
    FormArtistChanged(String),
    FormInstrumentChanged(Instrument),
    FormYoutubeUrlChanged(String),
    FormSpotifyUrlChanged(String),
    FormPdfPathChanged(String),
    FormMp3PathChanged(String),

    // File picker
    FormPickPdf,
    FormPickMp3,
    FormPdfPicked(Option<String>),
    FormMp3Picked(Option<String>),

    // Library folder
    PickLibraryFolder,
    LibraryFolderPicked(Option<String>),

    // Form actions
    FormSubmit,
    FormCancel,

    // Pitch control
    PitchUp,
    PitchDown,
    PitchShiftReady { path: String, semitones: i32, samples: Vec<f32>, channels: u16, sample_rate: u32 },
    PitchShiftFailed(String),

    // Audio playback
    TogglePlayPause,
    PlayAudio,
    PauseAudio,
    ScrubAudio(f32),  // slider dragging: update display only
    SeekAudio(f32),   // slider released: perform the actual seek
    SkipAudio(f32),   // arrow key skip: positive = forward, negative = backward
    AudioTick,
    AudioError(String),

    // PDF viewer
    PdfRendered(Vec<std::path::PathBuf>),
    PdfError(String),
    ScrollPdf(f32),

    // External media links
    OpenUrl(String),

    // LLM tab sync
    AnalyzeSync,
    SyncAnalysisComplete(Vec<crate::model::sync_map::SyncPoint>),
    SyncAnalysisFailed(String),

    // Sync debug overlay
    DebugSync,
    SyncDebugReady(String),
    SyncDebugFailed(String),

    // Window
    WindowResized(u32),

    // Form focus
    FormTabPressed,

    // Detail panel toggle
    ToggleDetailPanel,
}
