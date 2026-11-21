//! The game screen: a puzzle, its saved progress, and how it is drawn.

use std::cell::RefCell;
use std::path::Path;

use dd_engine::{Game, PuzzleSpec};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::draw::Chrome;
use crate::data::library::Source;
use crate::data::progress::{self, puzzle_id, Progress};
use crate::data::records::Records;
use crate::look::{canvas_size, paint, Canvas, PaintOptions};
use crate::settings::Settings;
use crate::ui::board::{fitting_scale, BoardView};

/// Width of the side panel, in characters.
pub const PANEL_WIDTH: u16 = 26;
/// Height of the panel when it moves under the board.
const PANEL_HEIGHT: u16 = 6;

pub struct GameScreen {
    pub spec: PuzzleSpec,
    /// The puzzle's share code: the key for its progress and records.
    pub code: String,
    pub source: Source,
    pub progress: Progress,
    /// Set when the saved progress could not be read: saving is then off,
    /// so the file is never overwritten.
    pub not_saving: Option<String>,
    /// The last painted board, reused until something it shows changes.
    cache: RefCell<Option<(CacheKey, Canvas)>>,
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    anchors: Vec<Option<u8>>,
    scale: usize,
    theme: String,
    grain: u8,
    grid: bool,
    show_goal: bool,
}

/// Where the board and panel go.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Arrangement {
    pub board: Rect,
    pub panel: Rect,
    pub scale: usize,
    /// The board does not fit even at the smallest scale.
    pub cramped: bool,
}

impl GameScreen {
    /// Opens a puzzle, with its saved progress if there is any. The second
    /// value is a warning to show, if the progress could not be read.
    pub fn open(spec: PuzzleSpec, source: Source, data_dir: &Path) -> Result<(GameScreen, Option<String>), String> {
        let (puzzle, _) = spec.build().map_err(|e| e.to_string())?;
        let code = spec.to_code().map_err(|e| e.to_string())?;
        let (progress, not_saving) = match progress::load(data_dir, &code, &puzzle) {
            Ok(Some(saved)) => (saved, None),
            Ok(None) => (Progress::new(puzzle), None),
            Err(e) => (Progress::new(puzzle), Some(e)),
        };
        let warning = not_saving.as_ref().map(|e| format!("{e}; starting afresh and not saving over it"));
        let screen = GameScreen { spec, code, source, progress, not_saving, cache: RefCell::new(None) };
        Ok((screen, warning))
    }

    pub fn game(&self) -> &Game {
        &self.progress.game
    }

    pub fn title(&self) -> &str {
        if self.spec.name.is_empty() {
            "Untitled puzzle"
        } else {
            &self.spec.name
        }
    }

    pub fn id(&self) -> String {
        puzzle_id(&self.code)
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        if self.not_saving.is_some() {
            return Ok(());
        }
        progress::save(data_dir, &self.code, &self.progress)
    }

    /// Board beside the panel if it fits, else the panel underneath, else
    /// the board at the smallest scale, cut off.
    pub fn arrange(&self, settings: &Settings, area: Rect) -> Arrangement {
        let game = self.game();
        let theme = settings.theme();
        let beside_width = area.width.saturating_sub(PANEL_WIDTH + 2);
        if let Some(scale) = fitting_scale(game, theme, settings.scale, beside_width, area.height) {
            let board = Rect::new(area.x, area.y, beside_width, area.height);
            let panel = Rect::new(area.x + beside_width + 2, area.y, PANEL_WIDTH.min(area.width), area.height);
            return Arrangement { board, panel, scale, cramped: false };
        }
        let above = area.height.saturating_sub(PANEL_HEIGHT);
        let below = Rect::new(area.x, area.y + above, area.width, area.height - above);
        if let Some(scale) = fitting_scale(game, theme, settings.scale, area.width, above) {
            let board = Rect::new(area.x, area.y, area.width, above);
            return Arrangement { board, panel: below, scale, cramped: false };
        }
        let board = Rect::new(area.x, area.y, area.width, above);
        Arrangement { board, panel: below, scale: 4, cramped: true }
    }

    fn canvas(&self, settings: &Settings, scale: usize) -> Canvas {
        let game = self.game();
        let key = CacheKey {
            anchors: (0..game.block_count()).map(|b| game.anchor(b)).collect(),
            scale,
            theme: settings.theme.clone(),
            grain: settings.grain,
            grid: settings.grid,
            show_goal: settings.show_goal,
        };
        if let Some((cached, canvas)) = &*self.cache.borrow() {
            if *cached == key {
                return canvas.clone();
            }
        }
        let options = PaintOptions { scale, ..settings.paint_options() };
        let canvas = paint(game, settings.theme(), &options);
        *self.cache.borrow_mut() = Some((key, canvas.clone()));
        canvas
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, settings: &Settings, records: &Records, chrome: &Chrome) {
        let arrangement = self.arrange(settings, area);
        let canvas = self.canvas(settings, arrangement.scale);
        frame.render_widget(BoardView(&canvas), arrangement.board);

        let game = self.game();
        let best = records
            .get(&self.id())
            .and_then(|r| r.shown(settings.best))
            .map_or("—".to_string(), |s| s.to_string());
        let row = |name: &str, value: String| {
            Line::from(vec![Span::styled(format!("{name:<8}"), chrome.dim()), Span::styled(value, chrome.text())])
        };
        let mut lines = vec![
            Line::from(Span::styled(self.title().to_string(), chrome.title())),
            Line::default(),
            row("moves", game.move_count().to_string()),
            row("best", best),
            row("hints", self.progress.counters.hints.to_string()),
            row("solver", "—".into()),
        ];
        if arrangement.cramped {
            let (w, h) = canvas_size(game, settings.theme(), 4);
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(format!("window too small: the board needs {w}×{h}"), chrome.warn())));
        }
        if self.not_saving.is_some() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled("not saving progress", chrome.warn())));
        }
        frame.render_widget(Paragraph::new(lines).style(chrome.text()), arrangement.panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::home::testing::TempDir;

    fn klotski(dir: &Path) -> GameScreen {
        let spec = dd_engine::file::parse(include_str!("../../../puzzles/klotski.toml")).unwrap();
        GameScreen::open(spec, Source::BuiltIn { name: "klotski".into() }, dir).unwrap().0
    }

    #[test]
    fn panel_goes_beside_then_below_then_the_board_is_cut_off() {
        let tmp = TempDir::new();
        let screen = klotski(&tmp.0);
        let settings = Settings::default();
        let (w, h) = canvas_size(screen.game(), settings.theme(), 6);
        let (w, h) = (w as u16, h as u16);

        let wide = screen.arrange(&settings, Rect::new(0, 0, w + PANEL_WIDTH + 2, h));
        assert_eq!((wide.scale, wide.cramped), (6, false));
        assert_eq!(wide.panel.x, w + 2);

        let narrow = screen.arrange(&settings, Rect::new(0, 0, w, h + PANEL_HEIGHT));
        assert_eq!((narrow.scale, narrow.cramped), (6, false));
        assert_eq!(narrow.panel.y, h);

        let tiny = screen.arrange(&settings, Rect::new(0, 0, 10, 10));
        assert!(tiny.cramped);
    }

    #[test]
    fn the_board_is_painted_once_until_something_changes() {
        let tmp = TempDir::new();
        let screen = klotski(&tmp.0);
        let settings = Settings::default();
        let first = screen.canvas(&settings, 6);
        assert_eq!(screen.canvas(&settings, 6), first);
        let lighter = Settings { theme: "light".into(), ..Settings::default() };
        assert_ne!(screen.canvas(&lighter, 6), first);
    }
}
