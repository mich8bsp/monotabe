use std::time::Duration;

use iced::widget::text::Shaping;
use iced::widget::{button, column, row, slider, text};
use iced::{Alignment, Element, Length};

use crate::message::Message;

#[derive(Clone, Copy)]
pub struct MediaBarState {
    pub playing: bool,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub loaded: bool,
    pub slider_pos: f32, // seek_target while dragging, otherwise == position as secs
    pub pitch_semitones: i32,
}

pub fn view(state: MediaBarState) -> Element<'static, Message> {
    if !state.loaded {
        return text("No audio loaded").size(12).into();
    }

    let pos_secs = state.position.as_secs_f32();
    let dur_secs = state.duration.map(|d| d.as_secs_f32()).unwrap_or(0.0);
    let slider_max = dur_secs.max(pos_secs + 1.0);
    let slider_pos = state.slider_pos;

    let play_btn = if state.playing {
        button(text("⏸ Pause").shaping(Shaping::Advanced)).on_press(Message::PauseAudio)
    } else {
        button(text("▶ Play").shaping(Shaping::Advanced)).on_press(Message::PlayAudio)
    };

    let time_str = format!(
        "{} / {}",
        fmt_dur(state.position),
        state.duration.map(fmt_dur).unwrap_or_else(|| "--:--".to_string())
    );

    let seek = slider(0.0f32..=slider_max, slider_pos, Message::ScrubAudio)
        .on_release(Message::SeekAudio(slider_pos))
        .step(0.5f32)
        .width(Length::Fill);

    let pitch_label = match state.pitch_semitones {
        0 => "0 st".to_string(),
        n if n > 0 => format!("+{n} st"),
        n => format!("{n} st"),
    };

    let pitch_row = row![
        button(text("-").size(13))
            .on_press(Message::PitchDown)
            .padding([4, 10])
            .style(iced::theme::Button::Secondary),
        text(pitch_label).size(13),
        button(text("+").size(13))
            .on_press(Message::PitchUp)
            .padding([4, 10])
            .style(iced::theme::Button::Secondary),
    ]
    .spacing(4)
    .align_items(Alignment::Center);

    column![
        row![
            play_btn.padding([6, 14]),
            text(time_str).size(13),
            pitch_row,
        ]
        .spacing(12)
        .align_items(Alignment::Center),
        seek,
    ]
    .spacing(6)
    .into()
}

fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}
