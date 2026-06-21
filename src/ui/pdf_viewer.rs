use std::path::PathBuf;

use iced::widget::image::{self, Handle};
use iced::widget::{container, scrollable, text, Column};
use iced::{Element, Length};

use crate::message::Message;

pub fn viewer_id() -> scrollable::Id {
    scrollable::Id::new("pdf-viewer")
}

pub fn view(pages: &[PathBuf], rendering: bool) -> Element<'_, Message> {
    if rendering {
        return container(text("Rendering PDF…").size(14))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into();
    }

    if pages.is_empty() {
        return container(text("No tab PDF loaded").size(14))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into();
    }

    let page_images = pages.iter().fold(Column::new().spacing(4), |col, path| {
        col.push(
            image::Image::new(Handle::from_path(path))
                .width(Length::Fill),
        )
    });

    scrollable(
        container(page_images)
            .width(Length::Fill)
            .padding(8),
    )
    .id(viewer_id())
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
