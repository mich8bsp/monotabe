use std::time::Duration;

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
        button("⏸ Pause").on_press(Message::PauseAudio)
    } else {
        button("▶ Play").on_press(Message::PlayAudio)
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

    column![
        row![
            play_btn.padding([6, 14]),
            text(time_str).size(13),
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
