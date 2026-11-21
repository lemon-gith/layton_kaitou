//! ratatui pieces: the board widget and helpers shared by every screen.

pub mod board;

use ratatui::style::Color;

use crate::look::Rgb;

pub fn colour(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}
