mod app;
mod audio;
mod db;
mod llm;
mod message;
mod model;
mod pdf;
mod ui;
mod webview;

use app::Monotabe;
use iced::{Application, Settings};

fn main() -> iced::Result {
    Monotabe::run(Settings::default())
}
