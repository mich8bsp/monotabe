use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{Element, Length};

use crate::message::Message;
use crate::model::song::{Instrument, InstrumentFilter, Song};

pub fn view<'a>(
    songs: &'a [Song],
    filter: &'a InstrumentFilter,
    search: &'a str,
    selected_id: Option<&'a str>,
) -> Element<'a, Message> {
    let filter_bar = row![
        filter_btn("All", InstrumentFilter::All, filter),
        filter_btn("Guitar", InstrumentFilter::Guitar, filter),
        filter_btn("Bass", InstrumentFilter::Bass, filter),
    ]
    .spacing(4);

    let search_box = text_input("Search songs…", search)
        .on_input(Message::SearchChanged)
        .padding(6);

    let filtered: Vec<&Song> = songs
        .iter()
        .filter(|s| match filter {
            InstrumentFilter::All => true,
            InstrumentFilter::Guitar => s.instrument == Instrument::Guitar,
            InstrumentFilter::Bass => s.instrument == Instrument::Bass,
        })
        .filter(|s| {
            let q = search.to_lowercase();
            q.is_empty()
                || s.title.to_lowercase().contains(&q)
                || s.artist.to_lowercase().contains(&q)
        })
        .collect();

    let list = filtered.iter().fold(Column::new().spacing(2), |col, song| {
        let is_selected = selected_id == Some(song.id.as_str());
        let label = column![
            text(&song.title).size(14),
            text(format!("{} · {}", song.artist, song.instrument)).size(11),
        ]
        .spacing(2);
        let btn = button(label)
            .width(Length::Fill)
            .padding([6, 8])
            .on_press(Message::SongSelected(song.id.clone()))
            .style(if is_selected {
                iced::theme::Button::Primary
            } else {
                iced::theme::Button::Text
            });
        col.push(btn)
    });

    let add_btn = button("+ Add Song")
        .width(Length::Fill)
        .on_press(Message::NewSong)
        .style(iced::theme::Button::Secondary);

    container(
        column![
            filter_bar,
            search_box,
            scrollable(list).height(Length::Fill),
            add_btn,
        ]
        .spacing(8)
        .padding(8)
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn filter_btn<'a>(
    label: &'a str,
    value: InstrumentFilter,
    current: &'a InstrumentFilter,
) -> Element<'a, Message> {
    let active = &value == current;
    button(text(label).size(13))
        .padding([4, 10])
        .on_press(Message::FilterChanged(value))
        .style(if active {
            iced::theme::Button::Primary
        } else {
            iced::theme::Button::Secondary
        })
        .into()
}
