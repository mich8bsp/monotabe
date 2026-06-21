# Monotabe — TODO

## Current Status
**ALL PHASES COMPLETE** — app builds and is functionally complete

## Phases

### Phase 1 — Foundation [DONE]
- [x] Cargo.toml (iced 0.12, rusqlite bundled, serde, uuid, dirs)
- [x] src/main.rs
- [x] src/app.rs — bare iced Application skeleton, window opens with title
- [x] src/message.rs — Message enum
- [x] src/model/song.rs — Song struct, Instrument enum
- [x] src/model/media.rs — MediaSource enum
- [x] src/model/sync_map.rs — TabSyncMap, SyncPoint, scroll_position_at()
- [x] src/db/store.rs — SQLite via rusqlite; open/migrate/CRUD for songs + sync_maps
- [x] cargo check passes (warnings only — expected for unused code at this stage)

---

### Phase 2 — Library UI [DONE]
- [x] src/ui/mod.rs
- [x] src/ui/library.rs — scrollable song list, guitar/bass filter buttons, search box
- [x] src/ui/song_form.rs — add/edit form with file pickers
- [x] src/ui/song_detail.rs — detail view with edit/delete buttons
- [x] Wire app.rs: two-panel layout, load songs on startup, CRUD handlers
- [x] rfd crate for native file dialog (PDF/MP3 path pickers)
- [x] Full Message enum for Phase 2
- [x] cargo build passes

---

### Phase 3 — MP3 Playback [DONE]
- [x] rodio 0.22 API (Player + MixerDeviceSink + get_pos())
- [x] src/audio/player.rs — AudioPlayer with play/pause/seek/position
- [x] src/ui/media_bar.rs — play/pause button, seek slider, time display
- [x] subscription() polling AudioTick every 100ms
- [x] Auto-load MP3 on song select
- [x] Media bar shown in song_detail when mp3_path is set
- [x] cargo check passes


---

### Phase 4 — PDF Viewer [DONE]
- [x] iced "image" feature enabled
- [x] src/pdf/renderer.rs — async pdftoppm rendering to temp dir PNGs
- [x] src/ui/pdf_viewer.rs — scrollable image stack with scrollable::Id for scroll_to
- [x] PdfRendered/PdfError messages; auto-render on song select
- [x] song_detail layout: top metadata/controls + bottom PDF viewer (Fill height)
- [x] Note: requires pdftoppm (install poppler-utils)
- [x] cargo check passes


---

### Phase 5 — Media Links [DONE]
- [x] YouTube / Spotify buttons in song_detail.rs
- [x] OpenUrl(String) message
- [x] webkit2gtk companion window (src/webview/gtk_window.rs) — GTK runs in a background thread; iced communicates via glib channel
- [x] Window hides on close and is reused on subsequent opens
- [x] Lazy-init: GTK thread spawned on first click (after winit has called XInitThreads)
- [x] Fallback to system browser if GTK init fails
- [x] cargo build passes


---

### Phase 6 — LLM Tab Sync (Advanced) [DONE]
- [x] reqwest (json feature) + base64 in Cargo.toml
- [x] src/llm/mod.rs
- [x] src/llm/claude.rs — render PDF at 72 DPI via pdftoppm; base64-encode PNGs; POST /v1/messages with vision; parse JSON SyncPoints (y_frac)
- [x] "Analyze Sync" button in song_detail.rs (enabled when PDF + MP3 both set)
- [x] "✓ Sync active" badge + "Re-analyze Sync" when map exists
- [x] AnalyzeSync / SyncAnalysisComplete / SyncAnalysisFailed message variants
- [x] Auto-scroll: AudioTick → sync_map.scroll_position_at(t) → scrollable::scroll_to
- [x] png_height() reads PNG IHDR bytes for correct page-height tracking
- [x] API key from ANTHROPIC_API_KEY env var
- [x] cargo build passes (5 warnings — all intentional dead scaffolding)

---

## Runtime Requirements
- **poppler-utils**: `sudo apt install poppler-utils` (provides `pdftoppm`)
- **webkit2gtk + GTK3**: `sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev` (for YouTube/Spotify companion window)
- **ANTHROPIC_API_KEY**: must be set in environment for Analyze Sync to work
- **Audio output device**: required for MP3 playback

## Notes
- DB path: `~/.local/share/monotabe/library.db` (created on first run)
- iced version: 0.12.1 (Application trait API — not the newer functional style)
- rodio 0.22 API: `DeviceSinkBuilder` + `Player` (not the older `Sink`/`OutputStream`)
- Cloud sync: intentionally deferred; schema uses UUID PKs + updated_at timestamps
- YouTube/Spotify open in a webkit2gtk companion window (960×600, no browser chrome); GTK runs on a dedicated background thread communicating via glib channel

## Possible Future Enhancements
- Cloud sync backend (schema already sync-ready with UUIDs + updated_at)
- Settings screen (API key entry, model selection, default files dir)
- Sync accuracy improvement (re-analyze with claude-opus-4-8 for higher accuracy)
