use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Config;

use iced::widget::{container, row, scrollable, Rule};
use iced::{Application, Command, Element, Length, Subscription, Theme};

use crate::audio::player::AudioPlayer;
use crate::db::store::Store;
use crate::sync_gen;
use crate::message::Message;
use crate::model::song::{InstrumentFilter, Song};
use crate::model::sync_map::TabSyncMap;
use crate::pdf::renderer;
use crate::ui::media_bar::MediaBarState;
use crate::ui::pdf_viewer;
use crate::ui::{library, song_detail, song_form};
use crate::ui::song_form::SongFormState;
use crate::webview::gtk_window::WebviewHandle;

const PAGE_GAP_PX: f32 = 4.0;

pub struct Monotabe {
    config: Config,
    library_path: PathBuf,
    library_path_str: String,
    store: Store,
    songs: Vec<Song>,
    filter: InstrumentFilter,
    search: String,
    selected_song_id: Option<String>,
    form: Option<SongFormState>,
    confirm_delete_id: Option<String>,
    status: Option<String>,
    // Audio
    audio: Option<AudioPlayer>,
    // PDF viewer
    pdf_pages: Vec<PathBuf>,
    pdf_page_heights: Vec<f32>, // PNG pixel height
    pdf_page_widths: Vec<f32>,  // PNG pixel width
    pdf_rendering: bool,
    // Window width — used to compute displayed image heights for scroll math
    window_width: f32,
    // LLM sync
    sync_map: Option<TabSyncMap>,
    sync_analyzing: bool,
    // Seek scrubbing (slider drag target before mouse release)
    seek_target: Option<f32>,
    // Whether the song detail panel above the PDF is collapsed
    detail_collapsed: bool,
    // Pitch offset in semitones (applied at audio load time)
    pitch_semitones: i32,
    // True while a background pitch-shift task is running
    pitch_processing: bool,
    // Webview companion window (lazy-initialized on first OpenUrl)
    webview: Option<WebviewHandle>,
}

impl Application for Monotabe {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = Config::load();
        let library_path = config.effective_library_path();
        let library_path_str = library_path.to_string_lossy().into_owned();
        let store = Store::open(&library_path).expect("Failed to open database");
        let songs = store.all_songs().unwrap_or_default();
        (
            Self {
                config,
                library_path,
                library_path_str,
                store,
                songs,
                filter: InstrumentFilter::All,
                search: String::new(),
                selected_song_id: None,
                form: None,
                confirm_delete_id: None,
                status: None,
                audio: AudioPlayer::try_new(),
                pdf_pages: vec![],
                pdf_page_heights: vec![],
                pdf_page_widths: vec![],
                pdf_rendering: false,
                window_width: 1400.0,
                sync_map: None,
                sync_analyzing: false,
                seek_target: None,
                detail_collapsed: false,
                pitch_semitones: 0,
                pitch_processing: false,
                webview: None,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Monotabe".to_string()
    }

    fn theme(&self) -> Theme {
        Theme::KanagawaDragon
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = iced::event::listen_with(|event, status| {
            use iced::keyboard::key::Named;
            use iced::keyboard::{Event::KeyPressed, Key::Named as KNamed};
            match event {
                iced::Event::Window(_, iced::window::Event::Resized { width, .. }) => {
                    Some(Message::WindowResized(width))
                }
                iced::Event::Keyboard(KeyPressed { key: KNamed(Named::Tab), .. })
                    if status == iced::event::Status::Ignored =>
                {
                    Some(Message::FormTabPressed)
                }
                iced::Event::Keyboard(KeyPressed { key: KNamed(Named::Space), .. })
                    if status == iced::event::Status::Ignored =>
                {
                    Some(Message::TogglePlayPause)
                }
                iced::Event::Keyboard(KeyPressed { key: KNamed(Named::ArrowLeft), .. })
                    if status == iced::event::Status::Ignored =>
                {
                    Some(Message::SkipAudio(-10.0))
                }
                iced::Event::Keyboard(KeyPressed { key: KNamed(Named::ArrowRight), .. })
                    if status == iced::event::Status::Ignored =>
                {
                    Some(Message::SkipAudio(10.0))
                }
                _ => None,
            }
        });
        let has_audio = self.audio.as_ref().map(|a| a.is_loaded()).unwrap_or(false);
        if has_audio {
            Subscription::batch([
                events,
                iced::time::every(Duration::from_millis(100)).map(|_| Message::AudioTick),
            ])
        } else {
            events
        }
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        self.status = None;
        match message {
            // ── Library navigation ───────────────────────────────────────────
            Message::SongsLoaded(songs) => {
                self.songs = songs;
            }
            Message::SongSelected(id) => {
                self.selected_song_id = Some(id.clone());
                self.form = None;
                self.confirm_delete_id = None;
                self.pdf_pages = vec![];
                self.pdf_page_heights = vec![];
                self.pdf_rendering = false;
                self.sync_map = None;
                self.sync_analyzing = false;
                self.seek_target = None;
                self.pitch_processing = false;

                let mut cmds: Vec<Command<Message>> = vec![];

                if let Some(song) = self.songs.iter().find(|s| s.id == id) {
                    if let Some(path) = song.mp3_path.clone() {
                        if self.pitch_semitones == 0 {
                            if let Some(audio) = self.audio.as_mut() {
                                if let Err(e) = audio.load(path, 0) {
                                    self.status = Some(format!("Audio load failed: {e}"));
                                }
                            }
                        } else {
                            if let Some(audio) = self.audio.as_mut() { audio.stop(); }
                            self.pitch_processing = true;
                            let semitones = self.pitch_semitones;
                            cmds.push(Command::perform(
                                decode_and_shift(path, semitones),
                                map_pitch_result,
                            ));
                        }
                    } else if let Some(audio) = self.audio.as_mut() {
                        audio.stop();
                    }

                    self.sync_map = self.store.load_sync_map(&id).unwrap_or(None);

                    if let Some(pdf_path) = song.pdf_path.clone() {
                        self.pdf_rendering = true;
                        let sid = id.clone();
                        cmds.push(Command::perform(
                            renderer::render_pages(pdf_path, sid),
                            |r| match r {
                                Ok(pages) => Message::PdfRendered(pages),
                                Err(e) => Message::PdfError(e),
                            },
                        ));
                    }
                }

                return Command::batch(cmds);
            }
            Message::FilterChanged(f) => self.filter = f,
            Message::SearchChanged(s) => self.search = s,

            // ── CRUD ─────────────────────────────────────────────────────────
            Message::NewSong => {
                self.form = Some(SongFormState::new());
                return song_form::focus_first();
            }
            Message::EditSong(id) => {
                if let Some(song) = self.songs.iter().find(|s| s.id == id) {
                    self.form = Some(SongFormState::from_song(song));
                    return song_form::focus_first();
                }
            }
            Message::ConfirmDeleteSong(id) => {
                self.confirm_delete_id = Some(id);
            }
            Message::CancelDelete => {
                self.confirm_delete_id = None;
            }
            Message::DeleteSong(id) => {
                self.confirm_delete_id = None;
                match self.store.delete_song(&id) {
                    Ok(()) => {
                        self.songs.retain(|s| s.id != id);
                        if self.selected_song_id.as_deref() == Some(&id) {
                            self.selected_song_id = None;
                            self.pdf_pages = vec![];
                            self.sync_map = None;
                            if let Some(audio) = self.audio.as_mut() {
                                audio.stop();
                            }
                        }
                    }
                    Err(e) => self.status = Some(format!("Delete failed: {e}")),
                }
            }

            // ── Form field changes ────────────────────────────────────────────
            Message::FormTitleChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.title = v; }
            }
            Message::FormArtistChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.artist = v; }
            }
            Message::FormInstrumentChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.instrument = v; }
            }
            Message::FormYoutubeUrlChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.youtube_url = v; }
            }
            Message::FormSpotifyUrlChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.spotify_url = v; }
            }
            Message::FormPdfPathChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.pdf_path = v; }
            }
            Message::FormMp3PathChanged(v) => {
                if let Some(f) = self.form.as_mut() { f.mp3_path = v; }
            }

            // ── File pickers ──────────────────────────────────────────────────
            Message::FormPickPdf => {
                return Command::perform(
                    rfd::AsyncFileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .pick_file(),
                    |h| Message::FormPdfPicked(h.map(|f| f.path().to_string_lossy().to_string())),
                );
            }
            Message::FormPickMp3 => {
                return Command::perform(
                    rfd::AsyncFileDialog::new()
                        .add_filter("Audio", &["mp3", "m4a", "flac", "ogg", "wav"])
                        .pick_file(),
                    |h| Message::FormMp3Picked(h.map(|f| f.path().to_string_lossy().to_string())),
                );
            }
            Message::FormPdfPicked(path) => {
                if let Some(f) = self.form.as_mut() {
                    f.pdf_path = path.unwrap_or_default();
                }
            }
            Message::FormMp3Picked(path) => {
                if let Some(f) = self.form.as_mut() {
                    f.mp3_path = path.unwrap_or_default();
                }
            }

            // ── Form submit / cancel ──────────────────────────────────────────
            Message::FormSubmit => {
                if let Some(mut form) = self.form.take() {
                    if !form.is_valid() {
                        self.form = Some(form);
                        return Command::none();
                    }
                    // Copy files into library folder (skip if already there)
                    if !form.pdf_path.is_empty() {
                        match copy_to_library(&form.pdf_path, &self.library_path, "pdfs") {
                            Ok(p) => form.pdf_path = p,
                            Err(e) => {
                                self.status = Some(format!("PDF copy failed: {e}"));
                                self.form = Some(form);
                                return Command::none();
                            }
                        }
                    }
                    if !form.mp3_path.is_empty() {
                        match copy_to_library(&form.mp3_path, &self.library_path, "audio") {
                            Ok(p) => form.mp3_path = p,
                            Err(e) => {
                                self.status = Some(format!("Audio copy failed: {e}"));
                                self.form = Some(form);
                                return Command::none();
                            }
                        }
                    }
                    let song = form.to_song();
                    let is_new = form.editing_id.is_none();
                    let result = if is_new {
                        self.store.insert_song(&song)
                    } else {
                        self.store.update_song(&song)
                    };
                    match result {
                        Ok(()) => {
                            let sid = song.id.clone();
                            if is_new {
                                self.songs.push(song);
                            } else if let Some(existing) = self.songs.iter_mut().find(|s| s.id == sid) {
                                *existing = song;
                            }
                            self.songs.sort_by(|a, b| {
                                a.artist.cmp(&b.artist).then(a.title.cmp(&b.title))
                            });
                            self.selected_song_id = Some(sid);
                        }
                        Err(e) => {
                            self.status = Some(format!("Save failed: {e}"));
                            self.form = Some(form);
                        }
                    }
                }
            }
            Message::FormCancel => {
                self.form = None;
            }
            Message::FormTabPressed => {
                if let Some(form) = &mut self.form {
                    return song_form::tab_next_focus(&mut form.focused_field);
                }
            }

            // ── Pitch control ─────────────────────────────────────────────────
            Message::PitchUp => {
                self.pitch_semitones = (self.pitch_semitones + 1).min(12);
                if let Some(cmd) = self.start_pitch_reload() { return cmd; }
            }
            Message::PitchDown => {
                self.pitch_semitones = (self.pitch_semitones - 1).max(-12);
                if let Some(cmd) = self.start_pitch_reload() { return cmd; }
            }
            Message::PitchShiftReady { path, semitones, samples, channels, sample_rate } => {
                self.pitch_processing = false;
                // Discard stale results (user changed pitch or song while processing).
                let current_path = self.selected_song_id.as_ref()
                    .and_then(|id| self.songs.iter().find(|s| &s.id == id))
                    .and_then(|s| s.mp3_path.as_deref());
                if semitones == self.pitch_semitones && current_path == Some(path.as_str()) {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.load_processed(path, samples, channels, sample_rate);
                    }
                }
            }
            Message::PitchShiftFailed(e) => {
                self.pitch_processing = false;
                self.status = Some(format!("Pitch shift failed: {e}"));
            }

            // ── Library folder ────────────────────────────────────────────────
            Message::PickLibraryFolder => {
                return Command::perform(
                    rfd::AsyncFileDialog::new()
                        .set_title("Choose library folder")
                        .pick_folder(),
                    |h| Message::LibraryFolderPicked(h.map(|f| f.path().to_string_lossy().to_string())),
                );
            }
            Message::LibraryFolderPicked(Some(path)) => {
                let new_lib = PathBuf::from(&path);
                match Store::open(&new_lib) {
                    Ok(store) => {
                        self.store = store;
                        self.library_path_str = new_lib.to_string_lossy().into_owned();
                        self.library_path = new_lib.clone();
                        self.config.library_path = Some(new_lib);
                        self.config.save();
                        self.songs = self.store.all_songs().unwrap_or_default();
                        self.selected_song_id = None;
                        self.pdf_pages = vec![];
                        self.sync_map = None;
                        if let Some(audio) = self.audio.as_mut() { audio.stop(); }
                        self.status = Some(format!("Library folder set to {path}"));
                    }
                    Err(e) => {
                        self.status = Some(format!("Could not open library at {path}: {e}"));
                    }
                }
            }
            Message::LibraryFolderPicked(None) => {}

            // ── External media links ──────────────────────────────────────────
            Message::OpenUrl(url) => {
                // Lazy-init the GTK webview thread on first use (winit has
                // already called XInitThreads by the time iced is running).
                if self.webview.is_none() {
                    match crate::webview::gtk_window::spawn() {
                        Ok(handle) => self.webview = Some(handle),
                        Err(e) => {
                            self.status = Some(format!("Webview unavailable: {e}"));
                            std::thread::spawn(move || { let _ = open::that(url); });
                            return Command::none();
                        }
                    }
                }
                self.webview.as_ref().unwrap().open(url);
            }

            // ── Audio playback ────────────────────────────────────────────────
            Message::TogglePlayPause => {
                if let Some(audio) = self.audio.as_mut() {
                    if audio.is_playing() { audio.pause(); } else { audio.play(); }
                }
            }
            Message::PlayAudio => {
                if let Some(audio) = self.audio.as_mut() { audio.play(); }
            }
            Message::PauseAudio => {
                if let Some(audio) = self.audio.as_mut() { audio.pause(); }
            }
            Message::ScrubAudio(secs) => {
                self.seek_target = Some(secs);
            }
            Message::SeekAudio(secs) => {
                // Keep seek_target set so the slider doesn't snap back while
                // rodio processes the seek. AudioTick will clear it once
                // audio.position() has caught up.
                self.seek_target = Some(secs);
                if let Some(audio) = self.audio.as_mut() {
                    audio.seek(Duration::from_secs_f32(secs));
                }
            }
            Message::SkipAudio(delta) => {
                if let Some(audio) = self.audio.as_mut() {
                    let duration = audio.duration.map(|d| d.as_secs_f32()).unwrap_or(f32::MAX);
                    let current = self.seek_target.unwrap_or_else(|| audio.position().as_secs_f32());
                    let target = (current + delta).clamp(0.0, duration);
                    self.seek_target = Some(target);
                    audio.seek(Duration::from_secs_f32(target));
                }
            }
            Message::AudioTick => {
                if let Some(audio) = self.audio.as_ref() {
                    if audio.has_finished() {
                        if let Some(a) = self.audio.as_mut() { a.pause(); }
                        return Command::none();
                    }
                    // Clear seek_target once audio.position() has caught up
                    // (within 1s tolerance to account for seek latency).
                    if let Some(target) = self.seek_target {
                        if (audio.position().as_secs_f32() - target).abs() < 1.0 {
                            self.seek_target = None;
                        }
                    }
                    // Auto-scroll PDF when sync map present and audio playing
                    if audio.is_playing() {
                        if let Some(sync_map) = &self.sync_map {
                            let pos_secs = audio.position().as_secs_f32();
                            if let Some(y) = sync_scroll_y(
                                &sync_map.points,
                                pos_secs,
                                &self.pdf_page_heights,
                                &self.pdf_page_widths,
                                self.window_width,
                            ) {
                                return scrollable::scroll_to(
                                    pdf_viewer::viewer_id(),
                                    scrollable::AbsoluteOffset { x: 0.0, y },
                                );
                            }
                        }
                    }
                }
            }
            Message::AudioError(e) => {
                self.status = Some(e);
            }

            // ── PDF viewer ────────────────────────────────────────────────────
            Message::PdfRendered(pages) => {
                let dims: Vec<(f32, f32)> = pages.iter().map(|p| png_dimensions(p)).collect();
                self.pdf_page_widths  = dims.iter().map(|&(w, _)| w).collect();
                self.pdf_page_heights = dims.iter().map(|&(_, h)| h).collect();
                self.pdf_pages = pages;
                self.pdf_rendering = false;
            }
            Message::PdfError(e) => {
                self.pdf_rendering = false;
                self.status = Some(format!("PDF render failed: {e}"));
            }
            Message::ScrollPdf(y) => {
                return scrollable::scroll_to(
                    pdf_viewer::viewer_id(),
                    scrollable::AbsoluteOffset { x: 0.0, y },
                );
            }

            // ── Tab sync (image-based, no LLM) ───────────────────────────────
            Message::AnalyzeSync => {
                let song_id = match &self.selected_song_id {
                    Some(id) => id.clone(),
                    None => return Command::none(),
                };
                let song = match self.songs.iter().find(|s| s.id == song_id) {
                    Some(s) => s,
                    None => return Command::none(),
                };
                let (pdf_path, audio_dur) = match (&song.pdf_path, self.audio.as_ref()) {
                    (Some(p), Some(a)) => (p.clone(), a.duration),
                    _ => {
                        self.status = Some("Attach both a PDF and MP3 before analyzing".to_string());
                        return Command::none();
                    }
                };
                let dur_secs = audio_dur.map(|d| d.as_secs_f32()).unwrap_or(300.0);
                self.sync_analyzing = true;
                return Command::perform(
                    sync_gen::generate_simple_sync(pdf_path, song_id, dur_secs),
                    |r| match r {
                        Ok(pts) => Message::SyncAnalysisComplete(pts),
                        Err(e) => Message::SyncAnalysisFailed(e),
                    },
                );
            }
            Message::SyncAnalysisComplete(points) => {
                self.sync_analyzing = false;
                if let Some(id) = &self.selected_song_id {
                    let map = TabSyncMap {
                        song_id: id.clone(),
                        points,
                        model_used: "simple-equal-division".to_string(),
                    };
                    if let Err(e) = self.store.save_sync_map(&map) {
                        self.status = Some(format!("Failed to save sync map: {e}"));
                    } else {
                        self.sync_map = Some(map);
                        self.status = Some("Sync ready — auto-scroll active".to_string());
                    }
                }
            }
            Message::SyncAnalysisFailed(e) => {
                self.sync_analyzing = false;
                self.status = Some(format!("Sync analysis failed: {e}"));
            }

            // ── Sync debug overlay ────────────────────────────────────────────
            Message::DebugSync => {
                let points = match &self.sync_map {
                    Some(m) => m.points.clone(),
                    None => {
                        self.status = Some("No sync map — run Analyze Sync first".to_string());
                        return Command::none();
                    }
                };
                let song = self.selected_song_id.as_ref()
                    .and_then(|id| self.songs.iter().find(|s| &s.id == id));
                let song_title = song.map(|s| s.title.clone()).unwrap_or_default();
                let pdf_path = song.and_then(|s| s.pdf_path.clone());
                return Command::perform(
                    crate::debug::generate_sync_debug(
                        song_title,
                        self.pdf_pages.clone(),
                        self.pdf_page_heights.clone(),
                        points,
                        pdf_path,
                    ),
                    |r| match r {
                        Ok(path) => Message::SyncDebugReady(path),
                        Err(e) => Message::SyncDebugFailed(e),
                    },
                );
            }
            Message::SyncDebugReady(path) => {
                if let Err(e) = open::that(&path) {
                    self.status = Some(format!("Could not open debug file: {e}"));
                }
            }
            Message::SyncDebugFailed(e) => {
                self.status = Some(format!("Debug export failed: {e}"));
            }

            Message::WindowResized(w) => {
                self.window_width = w as f32;
            }
            Message::ToggleDetailPanel => {
                self.detail_collapsed = !self.detail_collapsed;
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let left_panel = library::view(
            &self.songs,
            &self.filter,
            &self.search,
            self.selected_song_id.as_deref(),
            &self.library_path_str,
        );

        let right_panel: Element<'_, Message> = if let Some(form) = &self.form {
            song_form::view(form)
        } else if let Some(id) = &self.selected_song_id {
            if let Some(song) = self.songs.iter().find(|s| &s.id == id) {
                let bar = self.audio.as_ref().map(|a| {
                    let pos_secs = a.position().as_secs_f32();
                    MediaBarState {
                        playing: a.is_playing(),
                        position: a.position(),
                        duration: a.duration,
                        loaded: a.is_loaded() || self.pitch_processing,
                        slider_pos: self.seek_target.unwrap_or(pos_secs),
                        pitch_semitones: self.pitch_semitones,
                        pitch_processing: self.pitch_processing,
                    }
                });
                song_detail::view(
                    song,
                    bar,
                    &self.pdf_pages,
                    self.pdf_rendering,
                    self.sync_map.is_some(),
                    self.sync_analyzing,
                    self.confirm_delete_id.as_deref() == Some(&song.id),
                    self.detail_collapsed,
                )
            } else {
                placeholder("Select a song")
            }
        } else {
            placeholder("Select a song from the library, or add one with '+ Add Song'")
        };

        let main_row = row![
            container(left_panel).width(Length::Fixed(280.0)).height(Length::Fill),
            Rule::vertical(1),
            container(right_panel).width(Length::Fill).height(Length::Fill),
        ];

        if let Some(msg) = &self.status {
            container(
                iced::widget::column![main_row, iced::widget::text(msg).size(12)]
                    .height(Length::Fill),
            )
            .into()
        } else {
            container(main_row)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }
}

/// Interpolate sync points in absolute-Y space so cross-page transitions scroll
/// smoothly (the pages are a continuous vertical strip in the scrollable widget).
fn sync_scroll_y(
    points: &[crate::model::sync_map::SyncPoint],
    time_secs: f32,
    page_heights: &[f32],
    page_widths: &[f32],
    window_width: f32,
) -> Option<f32> {
    if points.is_empty() {
        return None;
    }
    let idx = points.partition_point(|p| p.time_secs <= time_secs);
    if idx == 0 {
        let p = &points[0];
        return Some(absolute_y_of(page_heights, page_widths, window_width, p.page as usize, p.y_offset_px));
    }
    if idx >= points.len() {
        let p = &points[points.len() - 1];
        return Some(absolute_y_of(page_heights, page_widths, window_width, p.page as usize, p.y_offset_px));
    }
    let before = &points[idx - 1];
    let after  = &points[idx];
    let y0 = absolute_y_of(page_heights, page_widths, window_width, before.page as usize, before.y_offset_px);
    let y1 = absolute_y_of(page_heights, page_widths, window_width, after.page  as usize, after.y_offset_px);
    let t = ((time_secs - before.time_secs) / (after.time_secs - before.time_secs)).clamp(0.0, 1.0);
    Some(y0 + t * (y1 - y0))
}

// Width of UI chrome that sits to the left of (or inside) the PDF panel and is
// NOT part of the scrollable content area.  Used to estimate the displayed image
// width so that scroll positions are in iced's layout coordinate system.
//   280 library panel + 1 rule + 8+8 container padding = 297 px
const PANEL_CHROME_PX: f32 = 297.0;

/// Absolute scroll Y for a given (page, y_frac).
/// Images are rendered with Length::Fill so their displayed height scales with
/// the available panel width.  We replicate that scaling here so that the scroll
/// targets land at the correct position in iced's layout.
fn absolute_y_of(
    page_heights: &[f32],
    page_widths: &[f32],
    window_width: f32,
    page: usize,
    y_frac: f32,
) -> f32 {
    let available_width = (window_width - PANEL_CHROME_PX).max(1.0);
    let displayed_height = |i: usize| -> f32 {
        let png_h = page_heights.get(i).copied().unwrap_or(1650.0);
        let png_w = page_widths.get(i).copied().unwrap_or(1.0).max(1.0);
        png_h * (available_width / png_w)
    };
    let offset: f32 = (0..page).map(|i| displayed_height(i) + PAGE_GAP_PX).sum();
    offset + y_frac * displayed_height(page)
}

impl Monotabe {
    /// Start a background pitch-shift task for the selected song. Returns a
    /// Command if a task was dispatched, or handles the 0-semitone case inline.
    fn start_pitch_reload(&mut self) -> Option<Command<Message>> {
        let path = self.selected_song_id.as_ref()
            .and_then(|id| self.songs.iter().find(|s| &s.id == id))
            .and_then(|s| s.mp3_path.clone())?;

        if self.pitch_semitones == 0 {
            if let Some(audio) = self.audio.as_mut() {
                if let Err(e) = audio.load(path, 0) {
                    self.status = Some(format!("Audio load failed: {e}"));
                }
            }
            return None;
        }

        if let Some(audio) = self.audio.as_mut() { audio.stop(); }
        self.pitch_processing = true;
        let semitones = self.pitch_semitones;
        Some(Command::perform(
            decode_and_shift(path, semitones),
            map_pitch_result,
        ))
    }

    fn absolute_y(&self, page: usize, y_frac: f32) -> f32 {
        absolute_y_of(&self.pdf_page_heights, &self.pdf_page_widths, self.window_width, page, y_frac)
    }
}

async fn decode_and_shift(path: String, semitones: i32) -> Result<(String, i32, Vec<f32>, u16, u32), String> {
    tokio::task::spawn_blocking(move || -> Result<(String, i32, Vec<f32>, u16, u32), String> {
        use rodio::Source;
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let source = rodio::Decoder::try_from(file).map_err(|e| e.to_string())?;
        let sample_rate = source.sample_rate().get();
        let channels = source.channels().get();
        let raw: Vec<f32> = source.collect();
        let samples = crate::audio::pitch::pitch_shift(&raw, channels, semitones);
        Ok((path, semitones, samples, channels, sample_rate))
    })
    .await
    .map_err(|e: tokio::task::JoinError| e.to_string())?
}

fn map_pitch_result(r: Result<(String, i32, Vec<f32>, u16, u32), String>) -> Message {
    match r {
        Ok((path, semitones, samples, channels, sample_rate)) => {
            Message::PitchShiftReady { path, semitones, samples, channels, sample_rate }
        }
        Err(e) => Message::PitchShiftFailed(e),
    }
}

/// Read PNG IHDR to get (width, height) in pixels. Falls back to (1240, 1650).
fn png_dimensions(path: &Path) -> (f32, f32) {
    (|| -> Option<(f32, f32)> {
        let mut file = std::fs::File::open(path).ok()?;
        let mut hdr = [0u8; 24];
        file.read_exact(&mut hdr).ok()?;
        if &hdr[0..8] != b"\x89PNG\r\n\x1a\n" { return None; }
        let w = u32::from_be_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]) as f32;
        let h = u32::from_be_bytes([hdr[20], hdr[21], hdr[22], hdr[23]]) as f32;
        Some((w, h))
    })()
    .unwrap_or((1240.0, 1650.0))
}

fn copy_to_library(src: &str, library_dir: &Path, subfolder: &str) -> Result<String, String> {
    let src_path = Path::new(src);
    if src_path.starts_with(library_dir) {
        return Ok(src.to_string());
    }
    let filename = src_path
        .file_name()
        .ok_or_else(|| format!("Invalid path: {src}"))?;
    let dest_dir = library_dir.join(subfolder);
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| format!("Cannot create {subfolder} dir: {e}"))?;
    let dest = dest_dir.join(filename);
    if !dest.exists() {
        std::fs::copy(src_path, &dest).map_err(|e| format!("Copy failed: {e}"))?;
    }
    Ok(dest.to_string_lossy().to_string())
}

fn placeholder(msg: &str) -> Element<'_, Message> {
    container(iced::widget::text(msg).size(14))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y()
        .into()
}
