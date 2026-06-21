use std::path::PathBuf;

use iced::widget::{button, column, container, row, text, Rule};
use iced::{Element, Length};

use crate::message::Message;
use crate::model::song::Song;
use crate::ui::media_bar::{self, MediaBarState};
use crate::ui::pdf_viewer;

pub fn view<'a>(
    song: &'a Song,
    audio: Option<MediaBarState>,
    pdf_pages: &'a [PathBuf],
    pdf_rendering: bool,
    has_sync_map: bool,
    sync_analyzing: bool,
    confirming_delete: bool,
) -> Element<'a, Message> {
    let actions = if confirming_delete {
        row![
            text("Delete this song?").size(14),
            button("Yes, delete")
                .on_press(Message::DeleteSong(song.id.clone()))
                .padding([6, 18])
                .style(iced::theme::Button::Destructive),
            button("Cancel")
                .on_press(Message::CancelDelete)
                .padding([6, 18])
                .style(iced::theme::Button::Secondary),
        ]
        .spacing(8)
        .align_items(iced::Alignment::Center)
    } else {
        row![
            button("Edit")
                .on_press(Message::EditSong(song.id.clone()))
                .padding([6, 18])
                .style(iced::theme::Button::Secondary),
            button("Delete")
                .on_press(Message::ConfirmDeleteSong(song.id.clone()))
                .padding([6, 18])
                .style(iced::theme::Button::Destructive),
        ]
        .spacing(8)
    };

    let mut top = column![
        text(&song.title).size(26),
        text(format!("{} · {}", song.artist, song.instrument)).size(14),
        actions,
    ]
    .spacing(10)
    .padding([16, 16, 8, 16]);

    // YouTube / Spotify open-in-browser buttons
    if song.youtube_url.is_some() || song.spotify_url.is_some() {
        let mut link_row = row![].spacing(8);
        if let Some(url) = &song.youtube_url {
            link_row = link_row.push(
                button("YouTube")
                    .on_press(Message::OpenUrl(url.clone()))
                    .padding([5, 14])
                    .style(iced::theme::Button::Secondary),
            );
        }
        if let Some(url) = &song.spotify_url {
            link_row = link_row.push(
                button("Spotify")
                    .on_press(Message::OpenUrl(url.clone()))
                    .padding([5, 14])
                    .style(iced::theme::Button::Secondary),
            );
        }
        top = top.push(link_row);
    }

    // Audio player bar
    if song.mp3_path.is_some() {
        if let Some(bar) = audio {
            top = top.push(Rule::horizontal(1));
            top = top.push(container(media_bar::view(bar)).padding([4, 0]));
        }
    }

    // Sync analysis button (only when both PDF and audio are attached)
    let can_analyze = song.pdf_path.is_some() && song.mp3_path.is_some();
    if can_analyze {
        let sync_label = if sync_analyzing {
            "Analyzing…"
        } else if has_sync_map {
            "Re-analyze Sync"
        } else {
            "Analyze Sync"
        };
        let sync_btn = if sync_analyzing {
            button(sync_label).padding([5, 14]).style(iced::theme::Button::Secondary)
        } else {
            button(sync_label)
                .on_press(Message::AnalyzeSync)
                .padding([5, 14])
                .style(iced::theme::Button::Primary)
        };
        let mut sync_row = row![sync_btn].spacing(8).align_items(iced::Alignment::Center);
        if has_sync_map && !sync_analyzing {
            sync_row = sync_row.push(text("Sync active").size(12));
            sync_row = sync_row.push(
                button("Debug Sync")
                    .on_press(Message::DebugSync)
                    .padding([5, 12])
                    .style(iced::theme::Button::Secondary),
            );
        }
        top = top.push(sync_row);
    }

    // PDF viewer below controls
    let content: Element<'_, Message> = if song.pdf_path.is_some() || pdf_rendering {
        column![
            container(top),
            Rule::horizontal(1),
            pdf_viewer::view(pdf_pages, pdf_rendering),
        ]
        .height(Length::Fill)
        .into()
    } else {
        column![container(top)].height(Length::Fill).into()
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
