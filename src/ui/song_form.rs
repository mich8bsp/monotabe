use iced::widget::{button, column, container, radio, row, text, text_input};
use iced::{Element, Length};
use uuid::Uuid;

use crate::message::Message;
use crate::model::song::{Instrument, Song};

pub struct SongFormState {
    pub editing_id: Option<String>,
    original_created_at: Option<i64>,
    pub title: String,
    pub artist: String,
    pub instrument: Instrument,
    pub youtube_url: String,
    pub spotify_url: String,
    pub pdf_path: String,
    pub mp3_path: String,
}

impl SongFormState {
    pub fn new() -> Self {
        Self {
            editing_id: None,
            original_created_at: None,
            title: String::new(),
            artist: String::new(),
            instrument: Instrument::Guitar,
            youtube_url: String::new(),
            spotify_url: String::new(),
            pdf_path: String::new(),
            mp3_path: String::new(),
        }
    }

    pub fn from_song(song: &Song) -> Self {
        Self {
            editing_id: Some(song.id.clone()),
            original_created_at: Some(song.created_at),
            title: song.title.clone(),
            artist: song.artist.clone(),
            instrument: song.instrument.clone(),
            youtube_url: song.youtube_url.clone().unwrap_or_default(),
            spotify_url: song.spotify_url.clone().unwrap_or_default(),
            pdf_path: song.pdf_path.clone().unwrap_or_default(),
            mp3_path: song.mp3_path.clone().unwrap_or_default(),
        }
    }

    pub fn to_song(&self) -> Song {
        let now = unix_now();
        Song {
            id: self.editing_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            title: self.title.trim().to_string(),
            artist: self.artist.trim().to_string(),
            instrument: self.instrument.clone(),
            youtube_url: non_empty(&self.youtube_url),
            spotify_url: non_empty(&self.spotify_url),
            pdf_path: non_empty(&self.pdf_path),
            mp3_path: non_empty(&self.mp3_path),
            created_at: self.original_created_at.unwrap_or(now),
            updated_at: now,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.title.trim().is_empty() && !self.artist.trim().is_empty()
    }
}

pub fn view(form: &SongFormState) -> Element<'_, Message> {
    let heading = if form.editing_id.is_some() {
        "Edit Song"
    } else {
        "Add Song"
    };

    let instrument_picker = row![
        radio(
            "Guitar",
            Instrument::Guitar,
            Some(form.instrument.clone()),
            Message::FormInstrumentChanged
        ),
        radio(
            "Bass",
            Instrument::Bass,
            Some(form.instrument.clone()),
            Message::FormInstrumentChanged
        ),
    ]
    .spacing(16);

    let pdf_row = row![
        text_input("PDF tab file path…", &form.pdf_path)
            .on_input(Message::FormPdfPathChanged)
            .padding(6)
            .width(Length::Fill),
        button("Browse…")
            .on_press(Message::FormPickPdf)
            .padding([6, 12]),
    ]
    .spacing(6)
    .align_items(iced::Alignment::Center);

    let mp3_row = row![
        text_input("MP3 file path…", &form.mp3_path)
            .on_input(Message::FormMp3PathChanged)
            .padding(6)
            .width(Length::Fill),
        button("Browse…")
            .on_press(Message::FormPickMp3)
            .padding([6, 12]),
    ]
    .spacing(6)
    .align_items(iced::Alignment::Center);

    let submit_btn = if form.is_valid() {
        button("Save").on_press(Message::FormSubmit).style(iced::theme::Button::Primary)
    } else {
        button("Save").style(iced::theme::Button::Primary)
    };

    let actions = row![
        submit_btn.padding([8, 24]),
        button("Cancel")
            .on_press(Message::FormCancel)
            .padding([8, 24])
            .style(iced::theme::Button::Secondary),
    ]
    .spacing(8);

    let form_col = column![
        text(heading).size(22),
        labeled("Title *", text_input("Song title", &form.title).on_input(Message::FormTitleChanged).padding(6)),
        labeled("Artist *", text_input("Artist name", &form.artist).on_input(Message::FormArtistChanged).padding(6)),
        label_widget("Instrument", instrument_picker),
        labeled("YouTube URL", text_input("https://youtube.com/…", &form.youtube_url).on_input(Message::FormYoutubeUrlChanged).padding(6)),
        labeled("Spotify URL", text_input("https://open.spotify.com/…", &form.spotify_url).on_input(Message::FormSpotifyUrlChanged).padding(6)),
        label_widget("Tab PDF", pdf_row),
        label_widget("Audio MP3", mp3_row),
        actions,
    ]
    .spacing(14)
    .padding(24)
    .max_width(600);

    container(form_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn labeled<'a>(label: &'a str, input: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(12), input.into()]
        .spacing(4)
        .into()
}

fn label_widget<'a>(label: &'a str, widget: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(label).size(12), widget.into()]
        .spacing(4)
        .into()
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
