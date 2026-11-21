//! How boards look: colour themes and the painter that turns a game into a
//! grid of styled terminal characters.
//!
//! Boards are painted as small images first, two pixels per character (the
//! top and bottom halves of `▀`), which gives square pixels. That leaves
//! room for bevels, grain and gaps between blocks, and the same image works
//! for every theme. Labels are then written over the image as text.

// Parts of this are only used by the interactive interface, still to come.
#![allow(dead_code, unused_imports)]

pub mod paint;
pub mod theme;

pub use paint::{
    canvas_size, exit_opacity, exiting_block, paint, paint_moving, slide_millis, slide_offset, Canvas, Glyph, Motion, PaintOptions,
    EXIT_MILLIS,
};
pub use theme::{Rgb, Theme, THEMES};
