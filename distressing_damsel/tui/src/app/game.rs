//! The game screen: a puzzle, its saved progress, and how it is drawn.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use dd_engine::{Game, PuzzleSpec};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::draw::Chrome;
use super::play::{Animation, Mode};
use crate::data::library::Source;
use crate::data::progress::{self, puzzle_id, Progress};
use crate::data::records::Records;
use crate::look::{canvas_size, exit_opacity, exiting_block, paint, paint_moving, slide_offset, Canvas, Motion, PaintOptions, EXIT_MILLIS};
use crate::settings::{Labels, Settings};
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
    pub mode: Mode,
    /// Where the cursor was when selection last ended.
    pub last_cursor: Option<usize>,
    pub animation: Option<Animation>,
    /// The exit, waiting for the last slide to finish.
    pub pending_exit: Option<Animation>,
    /// The solved puzzle's key block has faded out.
    pub exited: bool,
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
    selected: Option<usize>,
    grabbed: bool,
    labels: bool,
    reach: u64,
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
        let exited = progress.game.is_solved();
        let screen = GameScreen {
            spec,
            code,
            source,
            progress,
            not_saving,
            mode: Mode::Base,
            last_cursor: None,
            animation: None,
            pending_exit: None,
            exited,
            cache: RefCell::new(None),
        };
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

    /// Board and panel side by side if they fit, else the panel under the
    /// board, else the board at the smallest scale, cut off. Board and panel
    /// are placed together as one group, centred in `area`.
    pub fn arrange(&self, settings: &Settings, area: Rect) -> Arrangement {
        let game = self.game();
        let theme = settings.theme();
        let size = |scale| {
            let (w, h) = canvas_size(game, theme, scale);
            (w as u16, h as u16)
        };
        let beside_width = area.width.saturating_sub(PANEL_WIDTH + 2);
        if let Some(scale) = fitting_scale(game, theme, settings.scale, beside_width, area.height) {
            let (w, h) = size(scale);
            let x = area.x + (area.width - (w + 2 + PANEL_WIDTH)) / 2;
            let y = area.y + (area.height - h) / 2;
            let board = Rect::new(x, y, w, h);
            let panel = Rect::new(x + w + 2, y, PANEL_WIDTH, area.y + area.height - y);
            return Arrangement { board, panel, scale, cramped: false };
        }
        let above = area.height.saturating_sub(PANEL_HEIGHT + 1);
        if let Some(scale) = fitting_scale(game, theme, settings.scale, area.width, above) {
            let (w, h) = size(scale);
            let x = area.x + (area.width - w) / 2;
            let y = area.y + (area.height - (h + 1 + PANEL_HEIGHT)) / 2;
            let board = Rect::new(x, y, w, h);
            let panel = Rect::new(x, y + h + 1, PANEL_WIDTH.max(w).min(area.width), PANEL_HEIGHT);
            return Arrangement { board, panel, scale, cramped: false };
        }
        let board = Rect::new(area.x, area.y, area.width, above);
        let panel = Rect::new(area.x, area.y + above + 1, area.width, area.height.saturating_sub(above + 1));
        Arrangement { board, panel, scale: 4, cramped: true }
    }

    /// Whether block labels show: always, never, or in jump mode.
    fn labels_shown(&self, settings: &Settings) -> bool {
        match settings.labels {
            Labels::Always => true,
            Labels::OnDemand => matches!(self.mode, Mode::Jump { .. }),
            Labels::Off => false,
        }
    }

    fn paint_options(&self, settings: &Settings, scale: usize) -> PaintOptions {
        let labels = if self.labels_shown(settings) {
            self.game().puzzle().blocks.iter().enumerate().map(|(i, b)| (i, b.label.to_string())).collect()
        } else {
            HashMap::new()
        };
        PaintOptions {
            scale,
            labels,
            selected: self.cursor(),
            grabbed: matches!(self.mode, Mode::Grab { .. }),
            reach: if settings.mark_reachable { self.reach() } else { 0 },
            ..settings.paint_options()
        }
    }

    /// The motion to draw now, if a block is moving, fading or gone.
    fn motion(&self, scale: usize, now: Instant) -> Option<Motion> {
        let elapsed = |started: &Instant| now.saturating_duration_since(*started).as_millis() as f32;
        match &self.animation {
            Some(Animation::Slide { block, path, started, millis }) => {
                let t = elapsed(started) / *millis as f32;
                Some(Motion { block: *block, offset: slide_offset(path, scale, t), opacity: 1.0 })
            }
            Some(Animation::Exit { block, started }) => {
                let t = elapsed(started) / EXIT_MILLIS as f32;
                Some(Motion { block: *block, offset: (0, 0), opacity: exit_opacity(t) })
            }
            None if self.exited => exiting_block(self.game()).map(|block| Motion { block, offset: (0, 0), opacity: 0.0 }),
            None => None,
        }
    }

    fn canvas(&self, settings: &Settings, scale: usize, now: Instant) -> Canvas {
        let game = self.game();
        let options = self.paint_options(settings, scale);
        if let Some(motion) = self.motion(scale, now) {
            return paint_moving(game, settings.theme(), &options, motion);
        }
        let key = CacheKey {
            anchors: (0..game.block_count()).map(|b| game.anchor(b)).collect(),
            scale,
            theme: settings.theme.clone(),
            grain: settings.grain,
            grid: settings.grid,
            show_goal: settings.show_goal,
            selected: options.selected,
            grabbed: options.grabbed,
            labels: !options.labels.is_empty(),
            reach: options.reach,
        };
        if let Some((cached, canvas)) = &*self.cache.borrow() {
            if *cached == key {
                return canvas.clone();
            }
        }
        let canvas = paint(game, settings.theme(), &options);
        *self.cache.borrow_mut() = Some((key, canvas.clone()));
        canvas
    }

    /// A line for the side panel saying what the keys do now.
    fn mode_line(&self) -> Option<String> {
        let label = |b: usize| self.game().puzzle().blocks[b].label;
        match self.mode {
            Mode::Base if self.game().is_solved() => Some("solved".into()),
            Mode::Base => None,
            Mode::Select { cursor } => Some(format!("on {}", label(cursor))),
            Mode::Grab { cursor, .. } => Some(format!("holding {}", label(cursor))),
            Mode::Jump { .. } => Some("type a label".into()),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, settings: &Settings, records: &Records, chrome: &Chrome) {
        let arrangement = self.arrange(settings, area);
        let canvas = self.canvas(settings, arrangement.scale, Instant::now());
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
        if let Some(mode) = self.mode_line() {
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(mode, chrome.selected())));
        }
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

        let narrow = screen.arrange(&settings, Rect::new(0, 0, w, h + PANEL_HEIGHT + 1));
        assert_eq!((narrow.scale, narrow.cramped), (6, false));
        assert_eq!(narrow.panel.y, h + 1);

        // With room to spare, board and panel stay together in the middle.
        let roomy = screen.arrange(&settings, Rect::new(0, 0, w + PANEL_WIDTH + 42, h + 10));
        assert_eq!(roomy.board.x, 20);
        assert_eq!(roomy.panel.x, roomy.board.x + w + 2);
        assert_eq!(roomy.board.y, 5);

        let tiny = screen.arrange(&settings, Rect::new(0, 0, 10, 10));
        assert!(tiny.cramped);
    }

    #[test]
    fn the_board_is_painted_once_until_something_changes() {
        let tmp = TempDir::new();
        let screen = klotski(&tmp.0);
        let settings = Settings::default();
        let now = Instant::now();
        let first = screen.canvas(&settings, 6, now);
        assert_eq!(screen.canvas(&settings, 6, now), first);
        let lighter = Settings { theme: "light".into(), ..Settings::default() };
        assert_ne!(screen.canvas(&lighter, 6, now), first);
    }
}
