# Monotabe

A native Linux desktop app for guitarists and bassists who practice with tabs. Load a PDF tab sheet and an MP3, hit play, and the tab scrolls automatically in sync with the music.

![Rust](https://img.shields.io/badge/Rust-stable-orange) ![Platform](https://img.shields.io/badge/platform-Linux-blue) ![License](https://img.shields.io/badge/license-MIT-green)

## Features

- **Song library** — store songs with title, artist, instrument (guitar / bass), links to YouTube and Spotify, and local paths to a PDF tab and MP3
- **PDF viewer** — renders tab PDFs page by page with smooth vertical scrolling
- **MP3 player** — play, pause, and seek with a scrub slider and elapsed/total time display
- **Auto-scroll sync** — detects tab row positions directly from the PDF images and divides the song duration equally across rows; the viewport holds steady for 75% of each row then smoothly slides to the next
- **YouTube / Spotify** — opens links in an embedded webkit2gtk browser window
- **Debug overlay** — exports an HTML file showing detected string lines, scroll anchors, and saved sync points overlaid on the PDF pages

## How sync works

1. The PDF is rendered at 100 DPI using `pdftoppm`
2. Each page image is scanned pixel-row by pixel-row; rows with ≥ 30 % dark coverage and a maximum gap ≤ 25 px between dark segments are classified as string lines
3. Consecutive string lines are grouped into staves; groups of exactly 6 (guitar) or 4 (bass) evenly-spaced lines are accepted as tab systems
4. A **scroll anchor** is placed above each system (chord names land at the top of the viewport) and a **string center** is recorded for visual reference
5. The audio duration is divided equally among all detected rows across all pages; two sync points are emitted per row — a snap at the row's start and a hold at 75 % through — so the last 25 % scrolls smoothly into the next row
6. During playback the scroll position is interpolated in absolute pixel space (not per-page fractions) so cross-page transitions are seamless

## Requirements

| Tool | Purpose |
|------|---------|
| `pdftoppm` (poppler-utils) | PDF → PNG rendering for the viewer and sync detector |
| GTK 3 + webkit2gtk | YouTube / Spotify companion window |
| A working audio output | MP3 playback via rodio |

```bash
# Ubuntu / Debian
sudo apt install poppler-utils libgtk-3-dev libwebkit2gtk-4.1-dev
```

## Building

```bash
git clone https://github.com/mich8bsp/monotabe.git
cd monotabe
cargo build --release
./target/release/monotabe
```

## Usage

1. Click **+ Add Song** and fill in the song details — attach a local PDF tab file and an MP3
2. Select the song from the library on the left
3. Click **Analyze Sync** — this takes a few seconds while the PDF pages are scanned
4. Press play; the tab scrolls automatically
5. Use the seek slider at any time — scrolling updates immediately
6. (Optional) Click **Debug Sync** to open an HTML overlay showing where the detector placed each sync point on the PDF

## Project structure

```
src/
├── app.rs           # iced Application — state, update, view
├── message.rs       # all Message variants
├── sync_gen.rs      # image-based sync map generation (no LLM required)
├── debug.rs         # HTML debug overlay generator
├── audio/           # rodio player wrapper
├── db/              # SQLite store (rusqlite)
├── llm/             # Anthropic API client (retained; not used for sync)
├── model/           # Song, SyncMap structs
├── pdf/
│   ├── renderer.rs  # pdftoppm → PNG pipeline
│   └── tab_detector.rs  # pixel-level stave detection
├── ui/              # iced widgets (library, song form, PDF viewer, media bar)
└── webview/         # webkit2gtk companion window
```

## Tech stack

- **[Rust](https://www.rust-lang.org/)** — language
- **[iced 0.12](https://github.com/iced-rs/iced)** — GUI (Elm-style, wgpu-rendered)
- **[rusqlite](https://github.com/rusqlite/rusqlite)** — local SQLite database
- **[rodio](https://github.com/RustAudio/rodio) + symphonia** — audio playback and decoding
- **[pdftoppm](https://poppler.freedesktop.org/)** — PDF rendering
- **[webkit2gtk](https://webkitgtk.org/)** — embedded browser for media links
- **[image](https://github.com/image-rs/image)** — PNG pixel analysis for tab detection
