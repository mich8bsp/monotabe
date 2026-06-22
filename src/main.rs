mod app;
mod audio;
mod config;
mod db;
mod debug;
mod llm;
mod message;
mod model;
mod pdf;
mod sync_gen;
mod ui;
mod webview;

use app::Monotabe;
use iced::{Application, Settings};

fn main() -> iced::Result {
    Monotabe::run(Settings::default())
}
