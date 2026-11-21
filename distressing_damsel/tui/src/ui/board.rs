//! Drawing a painted board into ratatui, and choosing a scale that fits.

use dd_engine::Game;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::widgets::Widget;

use super::colour;
use crate::look::{canvas_size, Canvas, Theme};

/// Draws a canvas centred in the area it is given. Parts that do not fit
/// are cut off.
pub struct BoardView<'a>(pub &'a Canvas);

impl Widget for BoardView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let canvas = self.0;
        let x0 = area.x + area.width.saturating_sub(canvas.width as u16) / 2;
        let y0 = area.y + area.height.saturating_sub(canvas.height as u16) / 2;
        for row in 0..canvas.height.min(area.height as usize) {
            for col in 0..canvas.width.min(area.width as usize) {
                let g = canvas.glyphs[row * canvas.width + col];
                if let Some(cell) = buf.cell_mut((x0 + col as u16, y0 + row as u16)) {
                    cell.reset();
                    cell.set_char(g.ch).set_fg(colour(g.fg)).set_bg(colour(g.bg));
                    if g.bold {
                        cell.modifier.insert(Modifier::BOLD);
                    }
                }
            }
        }
    }
}

/// The largest scale, from `preferred` down to 4 in steps of 2, at which
/// the board fits in `width` by `height` characters. `None` if it does not
/// fit even at 4.
pub fn fitting_scale(game: &Game, theme: &Theme, preferred: usize, width: u16, height: u16) -> Option<usize> {
    let mut scale = preferred.max(4) & !1;
    loop {
        let (w, h) = canvas_size(game, theme, scale);
        if w <= width as usize && h <= height as usize {
            return Some(scale);
        }
        if scale == 4 {
            return None;
        }
        scale -= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::{paint, PaintOptions, THEMES};
    use dd_engine::PuzzleSpec;

    fn klotski() -> Game {
        let spec = dd_engine::file::parse(include_str!("../../../puzzles/klotski.toml")).unwrap();
        Game::new(spec.build().unwrap().0)
    }

    #[test]
    fn scale_steps_down_to_fit() {
        let game = klotski();
        let theme = &THEMES[2];
        let (w6, h6) = canvas_size(&game, theme, 6);
        let (w4, h4) = canvas_size(&game, theme, 4);
        assert_eq!(fitting_scale(&game, theme, 6, w6 as u16, h6 as u16), Some(6));
        assert_eq!(fitting_scale(&game, theme, 8, w6 as u16, h6 as u16), Some(6));
        assert_eq!(fitting_scale(&game, theme, 6, w6 as u16 - 1, h6 as u16), Some(4));
        assert_eq!(fitting_scale(&game, theme, 6, w4 as u16, h4 as u16 - 1), None);
    }

    #[test]
    fn board_is_centred_and_clipped() {
        let mut spec = PuzzleSpec::from_grid("A.").unwrap();
        spec.goals.push(dd_engine::GoalSpec { block: 'A', x: 1, y: 0 });
        let game = Game::new(spec.build().unwrap().0);
        let canvas = paint(&game, &THEMES[0], &PaintOptions::default());
        let area = Rect::new(0, 0, canvas.width as u16 + 10, canvas.height as u16 + 4);
        let mut buf = Buffer::empty(area);
        BoardView(&canvas).render(area, &mut buf);
        // Five columns and two rows of margin before the board starts.
        assert_eq!(buf[(4, 2)].bg, ratatui::style::Color::Reset);
        assert_eq!(buf[(5, 2)].bg, colour(canvas.glyphs[0].bg));

        let small = Rect::new(0, 0, 3, 2);
        let mut buf = Buffer::empty(small);
        BoardView(&canvas).render(small, &mut buf);
        assert_eq!(buf[(0, 0)].bg, colour(canvas.glyphs[0].bg));
    }
}
