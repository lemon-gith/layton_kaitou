//! Paints a game onto a [`Canvas`].
//!
//! Motion is painted the same way as a still board: [`paint_moving`] draws
//! one block shifted by some pixels from where the game says it is, and/or
//! partly faded out. The interface calls it each frame with the offset and
//! opacity for that moment ([`slide_offset`] gives the offset along a drag),
//! so nothing is pre-rendered, and the terminal backend only redraws the
//! characters that changed.

use std::collections::{HashMap, HashSet};

use dd_engine::{bit, Dir, Game, Mask};

use super::theme::{grain, Rgb, Theme};

/// One terminal character.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
}

/// A rectangle of styled characters, ready for any terminal backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub glyphs: Vec<Glyph>,
}

impl Canvas {
    /// The canvas as 24-bit ANSI text, one line per row.
    pub fn to_ansi(&self) -> String {
        let mut out = String::new();
        for row in self.glyphs.chunks(self.width) {
            let mut last: Option<(Rgb, Rgb, bool)> = None;
            for g in row {
                if last != Some((g.fg, g.bg, g.bold)) {
                    out.push_str(&format!(
                        "\x1b[0;{}38;2;{};{};{};48;2;{};{};{}m",
                        if g.bold { "1;" } else { "" },
                        g.fg.0,
                        g.fg.1,
                        g.fg.2,
                        g.bg.0,
                        g.bg.1,
                        g.bg.2
                    ));
                    last = Some((g.fg, g.bg, g.bold));
                }
                out.push(g.ch);
            }
            out.push_str("\x1b[0m\n");
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct PaintOptions {
    /// Characters per cell across; cells are half as many rows tall, which
    /// is square in most terminals. Even, at least 4.
    pub scale: usize,
    /// Text to write on each block, by block id.
    pub labels: HashMap<usize, String>,
    /// Block to outline as selected.
    pub selected: Option<usize>,
    /// The selected block is held: its outline is drawn twice as thick.
    pub grabbed: bool,
    /// Cells to tint faintly, such as where a held block can go.
    pub reach: Mask,
    /// Outline where the key block has to go.
    pub show_goal: bool,
    /// Outline empty cells.
    pub grid: bool,
    /// Multiplies the theme's texture: 0 is smooth, 1 the theme's own.
    pub grain: f32,
}

impl Default for PaintOptions {
    fn default() -> Self {
        PaintOptions {
            scale: 6,
            labels: HashMap::new(),
            selected: None,
            grabbed: false,
            reach: 0,
            show_goal: false,
            grid: true,
            grain: 1.0,
        }
    }
}

/// One block drawn away from where the game has it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Motion {
    pub block: usize,
    /// Pixels from the block's position in the game: one pixel is one
    /// character across or half a character down.
    pub offset: (isize, isize),
    /// 1 is fully drawn, 0 is gone.
    pub opacity: f32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Owner {
    Frame,
    Floor,
    Wall,
    Face(usize),
}

impl Owner {
    fn raised(self) -> bool {
        !matches!(self, Owner::Floor)
    }
}

/// A painted board before it is packed into characters.
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<Rgb>,
    owner: Vec<Owner>,
    /// Pixels of the selection ring.
    ring: HashSet<usize>,
}

fn paint_image(game: &Game, theme: &Theme, options: &PaintOptions, hidden: Option<usize>) -> Image {
    let layout = game.layout();
    let s = options.scale.max(4) & !1;
    let (w, h) = (layout.width() as usize, layout.height() as usize);
    let (fs, ft, fb) = theme.frame_size;
    let (pw, ph) = (fs + 1 + w * s + fs, ft + 1 + h * s + fb);
    // One pixel of floor before the first cell matches the gap every cell
    // leaves after itself.
    let (left, top) = (fs + 1, ft + 1);
    let g = theme.grain * options.grain.max(0.0);

    let block_of: Vec<Option<usize>> = (0..layout.n_cells())
        .map(|c| game.block_at(c as u8).filter(|&b| Some(b) != hidden))
        .collect();
    let cell_kind = |cx: isize, cy: isize| -> Option<Result<usize, ()>> {
        if cx < 0 || cy < 0 || cx >= w as isize || cy >= h as isize {
            return None;
        }
        let cell = layout.cell(cx as u8, cy as u8);
        if layout.is_wall(cell) {
            Some(Err(()))
        } else {
            block_of[cell as usize].map(Ok)
        }
    };

    // Who owns each pixel.
    let mut owner = vec![Owner::Floor; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let o = &mut owner[y * pw + x];
            if x < fs || x >= pw - fs || y < ft || y >= ph - fb {
                *o = Owner::Frame;
                continue;
            }
            let bx = x as isize - left as isize;
            let by = y as isize - top as isize;
            let (cx, cy) = (bx.max(0) / s as isize, by.max(0) / s as isize);
            let (cx, cy) = (cx.min(w as isize - 1), cy.min(h as isize - 1));
            let kind = cell_kind(cx, cy);
            if kind == Some(Err(())) {
                *o = Owner::Wall;
                continue;
            }
            if bx < 0 || by < 0 {
                continue;
            }
            let Some(Ok(block)) = kind else { continue };
            let (lx, ly) = (bx as usize % s, by as usize % s);
            let same = |dx: isize, dy: isize| cell_kind(cx + dx, cy + dy) == Some(Ok(block));
            let right_gap = lx == s - 1 && !same(1, 0);
            let down_gap = ly == s - 1 && !same(0, 1);
            let corner_gap = lx == s - 1 && ly == s - 1 && !same(1, 1);
            if !(right_gap || down_gap || corner_gap) {
                *o = Owner::Face(block);
            }
        }
    }
    let at = |x: isize, y: isize| -> Owner {
        if x < 0 || y < 0 || x >= pw as isize || y >= ph as isize {
            Owner::Frame
        } else {
            owner[y as usize * pw + x as usize]
        }
    };

    let targets = layout.goal().targets();
    let goal_cells: Mask = targets
        .iter()
        .fold(0, |acc, &(class, anchor)| acc | layout.block_mask(class, anchor));
    let in_goal = |x: isize, y: isize| -> bool {
        let (bx, by) = (x - left as isize, y - top as isize);
        if bx < 0 || by < 0 || bx >= (w * s) as isize || by >= (h * s) as isize {
            return false;
        }
        let (cx, cy) = (bx as usize / s, by as usize / s);
        goal_cells & bit(layout.cell(cx as u8, cy as u8)) != 0
    };
    // The goal area inset the way a block's face is, so its outline sits
    // where the block's edge will.
    let goal_face = |x: isize, y: isize| -> bool {
        if !in_goal(x, y) {
            return false;
        }
        let (lx, ly) = ((x - left as isize) as usize % s, (y - top as isize) as usize % s);
        !(lx == s - 1 && !in_goal(x + 1, y)) && !(ly == s - 1 && !in_goal(x, y + 1))
    };

    let single_key = targets.len() == 1;
    let key_class = targets.first().map(|t| t.0);
    let colour_of_block = |block: usize| -> Rgb {
        let class = game.puzzle().blocks[block].class;
        if single_key && key_class == Some(class) {
            return theme.key;
        }
        let earlier = (0..class).filter(|&c| !single_key || Some(c) != key_class).count();
        theme.blocks[earlier % theme.blocks.len()]
    };

    // Each block face's bounding box, for gloss.
    let mut boxes: HashMap<usize, (usize, usize, usize)> = HashMap::new();
    if theme.gloss > 0.0 {
        for y in 0..ph {
            for x in 0..pw {
                if let Owner::Face(b) = owner[y * pw + x] {
                    let e = boxes.entry(b).or_insert((x, y, y));
                    *e = (e.0.min(x), e.1.min(y), e.2.max(y));
                }
            }
        }
    }

    let mut pixels = vec![theme.floor; pw * ph];
    for y in 0..ph {
        for x in 0..pw {
            let (xi, yi) = (x as isize, y as isize);
            let here = owner[y * pw + x];
            pixels[y * pw + x] = match here {
                Owner::Frame => {
                    // Mitred frame: grain follows the nearest outer edge.
                    let d = [x, pw - 1 - x, y, ph - 1 - y];
                    let side = (0..4).min_by_key(|&i| d[i]).unwrap();
                    let thick = [fs, fs, ft, fb][side];
                    let along = if side < 2 { y } else { x } as i32;
                    let mut c =
                        theme.frame.shade(grain(theme.frame_grain, along, d[side] as i32, side as u32 + 3) * 0.22 * g);
                    // Light from the top left: the outer edge catches it on
                    // the top and left, the inner edge on the bottom and right.
                    let lit_side = side == 0 || side == 2;
                    if d[side] == 0 {
                        c = c.shade(if lit_side { 0.6 } else { -0.6 } * theme.bevel);
                    } else if d[side] == thick - 1 {
                        c = c.shade(if lit_side { -0.9 } else { 0.5 } * theme.bevel);
                    } else if theme.carved {
                        c = carve(c, side, d, thick, along);
                    }
                    c
                }
                Owner::Wall => {
                    let mut c = theme.wall.shade(grain(theme.wall_grain, x as i32, y as i32, 11) * 0.22 * g);
                    let solid = |o: Owner| matches!(o, Owner::Wall | Owner::Frame);
                    if theme.bevel > 0.0 {
                        if !solid(at(xi, yi - 1)) || !solid(at(xi - 1, yi)) {
                            c = c.shade(theme.bevel * 0.6);
                        } else if !solid(at(xi, yi + 1)) || !solid(at(xi + 1, yi)) {
                            c = c.shade(-theme.bevel * 0.8);
                        }
                    }
                    c
                }
                Owner::Floor => {
                    let mut c = theme.floor.shade(grain(theme.floor_grain, x as i32, y as i32, 7) * 0.25 * g);
                    let (bx, by) = (xi - left as isize, yi - top as isize);
                    let grid_line = bx < 0 || by < 0 || bx as usize % s == s - 1 || by as usize % s == s - 1;
                    if options.grid && grid_line {
                        c = c.shade(if theme.dark { theme.floor_grid } else { -theme.floor_grid });
                    }
                    if options.reach != 0 && bx >= 0 && by >= 0 && (bx as usize) < w * s && (by as usize) < h * s {
                        let cell = layout.cell((bx as usize / s) as u8, (by as usize / s) as u8);
                        if options.reach & bit(cell) != 0 {
                            c = c.mix(theme.accent, 0.16);
                        }
                    }
                    if options.show_goal && goal_face(xi, yi) {
                        let edge = [(0, -1), (0, 1), (-1, 0), (1, 0)]
                            .iter()
                            .any(|&(dx, dy)| !goal_face(xi + dx, yi + dy));
                        c = c.mix(theme.goal, if edge { 0.7 } else { 0.12 });
                    }
                    if theme.shadow > 0.0 {
                        let cast = at(xi, yi - 1).raised() as u8 + at(xi - 1, yi).raised() as u8;
                        if cast > 0 {
                            c = c.shade(-theme.shadow * if cast == 2 { 1.2 } else { 1.0 });
                        } else if at(xi - 1, yi - 1).raised() {
                            c = c.shade(-theme.shadow * 0.5);
                        }
                    }
                    c
                }
                Owner::Face(block) => {
                    let seed = 101 + block as u32 * 7;
                    let mut c = colour_of_block(block).shade(grain(theme.block_grain, x as i32, y as i32, seed) * 0.25 * g);
                    if theme.bevel > 0.0 {
                        let lit = at(xi, yi - 1) != here || at(xi - 1, yi) != here;
                        let dim = at(xi, yi + 1) != here || at(xi + 1, yi) != here;
                        c = match (lit, dim) {
                            (true, false) => c.shade(theme.bevel),
                            (false, true) => c.shade(-theme.bevel * 1.3),
                            _ => c,
                        };
                    }
                    if let Some(&(x0, y0, y1)) = boxes.get(&block) {
                        c = gloss(c, theme.gloss, x - x0, y - y0, y1 - y0 + 1);
                    }
                    c
                }
            };
        }
    }

    // The exit: where a lone target meets the side it leaves by, the frame
    // takes the goal colour, except along its outer edge.
    if let (true, Some(dir)) = (single_key, game.puzzle().exit) {
        let (dx, dy) = dir.delta();
        for y in top..top + h * s {
            for x in left..left + w * s {
                if !goal_face(x as isize, y as isize) {
                    continue;
                }
                // Walk out through the gap and the frame.
                let (mut px, mut py) = (x as isize, y as isize);
                loop {
                    px += dx as isize;
                    py += dy as isize;
                    if px <= 0 || py <= 0 || px >= pw as isize - 1 || py >= ph as isize - 1 || goal_face(px, py) {
                        break;
                    }
                    let i = py as usize * pw + px as usize;
                    if owner[i] == Owner::Frame {
                        pixels[i] = pixels[i].mix(theme.goal, 0.8);
                    }
                }
            }
        }
    }

    // Selection: a ring just outside the block, twice as thick when held.
    let mut ring = HashSet::new();
    if let Some(sel) = options.selected.filter(|&b| Some(b) != hidden) {
        let face = |x: isize, y: isize| at(x, y) == Owner::Face(sel);
        let r: isize = if options.grabbed { 2 } else { 1 };
        for y in 0..ph as isize {
            for x in 0..pw as isize {
                let on_frame = at(x, y) == Owner::Frame;
                if !face(x, y) && !on_frame && (-r..=r).any(|dy| (-r..=r).any(|dx| face(x + dx, y + dy))) {
                    ring.insert(y as usize * pw + x as usize);
                }
            }
        }
        for &i in &ring {
            pixels[i] = theme.accent;
        }
    }

    Image { width: pw, height: ph, pixels, owner, ring }
}

/// Lacquer on a block face: a sheen fading down from the top and a glint
/// in the top-left corner. `fx`, `fy` are from the face's top-left.
fn gloss(c: Rgb, amount: f32, fx: usize, fy: usize, face_height: usize) -> Rgb {
    if amount <= 0.0 {
        return c;
    }
    let reach = (face_height as f32 * 0.45).max(2.0);
    let mut c = c;
    if (fy as f32) < reach {
        c = c.shade(amount * 0.5 * (1.0 - fy as f32 / reach));
    }
    match (fx, fy) {
        (1, 1) => c.shade(amount * 2.2),
        (2, 1) | (1, 2) => c.shade(amount * 1.4),
        (3, 1) | (1, 3) => c.shade(amount * 0.7),
        _ => c,
    }
}

/// Carving on a frame pixel that is not on its outer or inner edge: a groove
/// one pixel in with studs set along it, a bead before the inner edge, and
/// mitre lines at the corners.
fn carve(c: Rgb, side: usize, d: [usize; 4], thick: usize, along: i32) -> Rgb {
    let depth = d[side];
    // Mitre: the pixel is as close to a crossing side as to its own.
    let other = if side < 2 { d[2].min(d[3]) } else { d[0].min(d[1]) };
    if other == depth {
        return c.shade(-0.3);
    }
    if depth == 1 {
        return match along.rem_euclid(10) {
            5 => c.shade(0.32),
            4 | 6 => c.shade(-0.45),
            _ => c.shade(-0.28),
        };
    }
    if depth == thick - 2 && thick > 3 {
        return c.shade(0.12);
    }
    c
}

/// Where a label of `len` characters goes on a block: the run of whole
/// characters nearest the face's centre of mass. That point can be off the
/// block (an L's corner), so only runs entirely on the face count. Returns
/// the pixel column and character row.
fn label_spot(image: &Image, block: usize, len: usize) -> Option<(usize, usize)> {
    let pw = image.width;
    let face = |x: usize, y: usize| image.owner[y * pw + x] == Owner::Face(block);
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for y in 0..image.height {
        for x in 0..pw {
            if face(x, y) {
                sx += x as f32;
                sy += y as f32;
                n += 1.0;
            }
        }
    }
    if n == 0.0 || len == 0 {
        return None;
    }
    let (mx, my) = (sx / n, sy / n);
    let mut best: Option<(f32, usize, usize)> = None;
    for row in 0..image.height / 2 {
        for x in 0..pw.saturating_sub(len - 1) {
            if !(0..len).all(|i| face(x + i, row * 2) && face(x + i, row * 2 + 1)) {
                continue;
            }
            let dx = x as f32 + (len as f32 - 1.0) / 2.0 - mx;
            let dy = row as f32 * 2.0 + 0.5 - my;
            let d = dx * dx + dy * dy;
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, x, row));
            }
        }
    }
    best.map(|(_, x, row)| (x, row))
}

/// A label to write at a given character position.
struct Label<'a> {
    x: usize,
    row: usize,
    text: &'a str,
}

/// Packs pixels into characters and writes the labels.
fn to_canvas(width: usize, height: usize, pixels: &[Rgb], theme: &Theme, labels: &[Label]) -> Canvas {
    let (cw, ch) = (width, height / 2);
    let mut glyphs = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        for x in 0..cw {
            let (upper, lower) = (pixels[row * 2 * width + x], pixels[(row * 2 + 1) * width + x]);
            glyphs.push(if upper == lower {
                Glyph { ch: ' ', fg: upper, bg: upper, bold: false }
            } else {
                Glyph { ch: '▀', fg: upper, bg: lower, bold: false }
            });
        }
    }
    for label in labels {
        for (i, c) in label.text.chars().enumerate() {
            let x = label.x + i;
            if x >= cw || label.row >= ch {
                continue;
            }
            let bg = pixels[label.row * 2 * width + x].mix(pixels[(label.row * 2 + 1) * width + x], 0.5);
            let fg = if bg.luma() > 0.62 { theme.text_on_light } else { theme.text };
            glyphs[label.row * cw + x] = Glyph { ch: c, fg, bg, bold: true };
        }
    }
    Canvas { width: cw, height: ch, glyphs }
}

fn labels_for<'a>(image: &Image, labels: &'a HashMap<usize, String>, skip: Option<usize>) -> Vec<Label<'a>> {
    labels
        .iter()
        .filter(|(&b, _)| Some(b) != skip)
        .filter_map(|(&block, text)| {
            label_spot(image, block, text.chars().count()).map(|(x, row)| Label { x, row, text })
        })
        .collect()
}

/// Size in characters `(width, height)` of the canvas [`paint`] makes at
/// `scale`, without painting it.
pub fn canvas_size(game: &Game, theme: &Theme, scale: usize) -> (usize, usize) {
    let layout = game.layout();
    let s = scale.max(4) & !1;
    let (fs, ft, fb) = theme.frame_size;
    (fs + 1 + layout.width() as usize * s + fs, (ft + 1 + layout.height() as usize * s + fb) / 2)
}

pub fn paint(game: &Game, theme: &Theme, options: &PaintOptions) -> Canvas {
    let image = paint_image(game, theme, options, None);
    let labels = labels_for(&image, &options.labels, None);
    to_canvas(image.width, image.height, &image.pixels, theme, &labels)
}

/// Paints the board with one block shifted and/or faded; see [`Motion`].
/// The block, its selection ring and its label move together, and it casts
/// a shadow where it is drawn.
pub fn paint_moving(game: &Game, theme: &Theme, options: &PaintOptions, motion: Motion) -> Canvas {
    if motion.offset == (0, 0) && motion.opacity >= 1.0 {
        return paint(game, theme, options);
    }
    let full = paint_image(game, theme, options, None);
    let base = paint_image(game, theme, options, Some(motion.block));
    let (w, h) = (full.width as isize, full.height as isize);
    let opacity = motion.opacity.clamp(0.0, 1.0);
    let (ox, oy) = motion.offset;
    let inside = |x: isize, y: isize| x >= 0 && y >= 0 && x < w && y < h;

    // (x, y, colour, is face) for each pixel that travels with the block.
    let mut moved: Vec<(isize, isize, Rgb, bool)> = Vec::new();
    for y in 0..full.height {
        for x in 0..full.width {
            let i = y * full.width + x;
            let face = full.owner[i] == Owner::Face(motion.block);
            if face || (options.selected == Some(motion.block) && full.ring.contains(&i)) {
                moved.push((x as isize + ox, y as isize + oy, full.pixels[i], face));
            }
        }
    }
    let covered: HashSet<(isize, isize)> = moved.iter().map(|m| (m.0, m.1)).collect();
    let mut pixels = base.pixels.clone();
    // Shadow first, so the block is drawn over its own.
    if theme.shadow > 0.0 {
        let mut shaded = HashSet::new();
        for &(x, y, _, face) in &moved {
            for (sx, sy) in [(x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                if face && inside(sx, sy) && !covered.contains(&(sx, sy)) && shaded.insert((sx, sy)) {
                    let i = sy as usize * full.width + sx as usize;
                    if base.owner[i] == Owner::Floor {
                        pixels[i] = pixels[i].shade(-theme.shadow * opacity);
                    }
                }
            }
        }
    }
    for &(x, y, c, _) in &moved {
        if inside(x, y) {
            let i = y as usize * full.width + x as usize;
            pixels[i] = c.mix(pixels[i], 1.0 - opacity);
        }
    }

    let mut labels = labels_for(&base, &options.labels, Some(motion.block));
    if let Some(text) = options.labels.get(&motion.block).filter(|_| opacity >= 0.5) {
        if let Some((x, row)) = label_spot(&full, motion.block, text.chars().count()) {
            let (x, y) = (x as isize + ox, row as isize * 2 + oy);
            if x >= 0 && y >= 0 {
                labels.push(Label { x: x as usize, row: (y / 2) as usize, text });
            }
        }
    }
    to_canvas(full.width, full.height, &pixels, theme, &labels)
}

/// How long a slide along a drag path takes, in milliseconds: quick for one
/// cell, a little longer for long drags.
pub fn slide_millis(steps: usize) -> u64 {
    (70 + 55 * steps as u64).min(420)
}

/// The offset to give [`Motion`] at time `t` (0 to 1) of a slide along
/// `path`, a drag the game has already made. Starts at the block's old
/// position and eases into its new one, following corners.
pub fn slide_offset(path: &[Dir], scale: usize, t: f32) -> (isize, isize) {
    let s = (scale.max(4) & !1) as f32;
    let t = t.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    // Distance still to travel, in cells, walked back from the end.
    let mut left = (1.0 - eased) * path.len() as f32;
    let (mut x, mut y) = (0.0f32, 0.0f32);
    for dir in path.iter().rev() {
        if left <= 0.0 {
            break;
        }
        let part = left.min(1.0);
        let (dx, dy) = dir.delta();
        x -= dx as f32 * part * s;
        y -= dy as f32 * part * s;
        left -= part;
    }
    (x.round() as isize, y.round() as isize)
}

/// How long the key block's exit takes, in milliseconds.
pub const EXIT_MILLIS: u64 = 900;

/// Opacity at time `t` (0 to 1) of the key block's exit: it holds for a
/// moment, then fades.
pub fn exit_opacity(t: f32) -> f32 {
    let t = ((t - 0.25) / 0.75).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The block that leaves the box when the puzzle is solved: the only
/// target's block, on its target.
pub fn exiting_block(game: &Game) -> Option<usize> {
    if !game.is_solved() {
        return None;
    }
    let targets = game.layout().goal().targets();
    let &[(class, anchor)] = targets.as_slice() else {
        return None;
    };
    (0..game.block_count()).find(|&b| game.puzzle().blocks[b].class == class && game.anchor(b) == Some(anchor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::THEMES;
    use dd_engine::PuzzleSpec;

    fn game(grid: &str, goal: (char, u8, u8)) -> Game {
        let mut spec = PuzzleSpec::from_grid(grid).unwrap();
        spec.goals.push(dd_engine::GoalSpec { block: goal.0, x: goal.1, y: goal.2 });
        Game::new(spec.build().unwrap().0)
    }

    fn label_rows(canvas: &Canvas) -> Vec<String> {
        canvas
            .glyphs
            .chunks(canvas.width)
            .map(|r| r.iter().map(|g| if g.bold { g.ch } else { ' ' }).collect())
            .collect()
    }

    #[test]
    fn every_puzzle_paints_in_every_theme() {
        for (_, source) in crate::puzzles::BUILT_IN {
            let (puzzle, _) = dd_engine::file::parse(source).unwrap().build().unwrap();
            let game = Game::new(puzzle);
            let labels: HashMap<usize, String> = (0..game.block_count()).map(|b| (b, format!("{b}"))).collect();
            for theme in &THEMES {
                for scale in [4, 6, 8, 12] {
                    for (show_goal, grid) in [(false, true), (true, false)] {
                        let options = PaintOptions {
                            scale,
                            labels: labels.clone(),
                            selected: Some(0),
                            grabbed: grid,
                            reach: if show_goal { !0 } else { 0 },
                            show_goal,
                            grid,
                            grain: 1.5,
                        };
                        let canvas = paint(&game, theme, &options);
                        assert_eq!(canvas.glyphs.len(), canvas.width * canvas.height);
                        let rows = label_rows(&canvas);
                        for b in 0..game.block_count() {
                            let text = b.to_string();
                            assert!(rows.iter().any(|r| r.contains(&text)), "label {text} missing");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn canvas_size_matches_paint() {
        let game = game("A..\n...", ('A', 2, 1));
        for theme in &THEMES {
            for scale in [4, 6, 8] {
                let canvas = paint(&game, theme, &PaintOptions { scale, ..Default::default() });
                assert_eq!(canvas_size(&game, theme, scale), (canvas.width, canvas.height));
            }
        }
    }

    #[test]
    fn grabbed_blocks_and_reach_are_marked() {
        let game = game("A..\n...", ('A', 2, 1));
        let theme = &THEMES[0];
        let selected = PaintOptions { selected: Some(0), ..Default::default() };
        let held = PaintOptions { grabbed: true, ..selected.clone() };
        let accents = |c: &Canvas| c.glyphs.iter().filter(|g| g.fg == theme.accent || g.bg == theme.accent).count();
        assert!(accents(&paint(&game, theme, &held)) > accents(&paint(&game, theme, &selected)));
        let reach = PaintOptions { reach: bit(1) | bit(2), ..Default::default() };
        assert_ne!(paint(&game, theme, &reach), paint(&game, theme, &PaintOptions::default()));
    }

    #[test]
    fn goal_outline_and_grid_are_optional() {
        let game = game("A..\n...", ('A', 2, 1));
        let theme = &THEMES[0];
        let plain = paint(&game, theme, &PaintOptions::default());
        assert_ne!(plain, paint(&game, theme, &PaintOptions { show_goal: true, ..Default::default() }));
        assert_ne!(plain, paint(&game, theme, &PaintOptions { grid: false, ..Default::default() }));
    }

    #[test]
    fn grain_setting_scales_texture() {
        let game = game("A..\n...", ('A', 2, 1));
        let theme = &THEMES[2];
        let colours = |grain| {
            let canvas = paint(&game, theme, &PaintOptions { grain, ..Default::default() });
            canvas.glyphs.iter().map(|g| g.bg).collect::<HashSet<_>>().len()
        };
        assert!(colours(0.0) < colours(1.0));
    }

    #[test]
    fn motion_without_offset_or_fade_is_the_plain_board() {
        let game = game("AB.\n...", ('A', 2, 1));
        let options = PaintOptions {
            labels: HashMap::from([(0, "A".into()), (1, "B".into())]),
            selected: Some(0),
            ..Default::default()
        };
        for theme in &THEMES {
            let motion = Motion { block: 0, offset: (0, 0), opacity: 1.0 };
            assert_eq!(paint_moving(&game, theme, &options, motion), paint(&game, theme, &options));
        }
    }

    #[test]
    fn a_shifted_block_carries_its_label() {
        let game = game("A..\n...", ('A', 2, 1));
        let options = PaintOptions { labels: HashMap::from([(0, "A".into())]), ..Default::default() };
        let theme = &THEMES[0];
        let find = |c: &Canvas| c.glyphs.iter().position(|g| g.ch == 'A').unwrap();
        let still = paint(&game, theme, &options);
        let moved = paint_moving(&game, theme, &options, Motion { block: 0, offset: (3, 0), opacity: 1.0 });
        assert_eq!(find(&moved), find(&still) + 3);
        let faded = paint_moving(&game, theme, &options, Motion { block: 0, offset: (0, 0), opacity: 0.2 });
        assert!(faded.glyphs.iter().all(|g| g.ch != 'A'));
    }

    #[test]
    fn a_faded_out_block_looks_like_empty_floor() {
        for theme in &THEMES {
            // Grid off and no texture, so every empty floor pixel inside the
            // board is the same colour except for shadows.
            let options = PaintOptions { grid: false, grain: 0.0, ..Default::default() };
            let game = game("...\n.A.\n...", ('A', 1, 1));
            let gone = paint_moving(&game, theme, &options, Motion { block: 0, offset: (0, 0), opacity: 0.0 });
            let (fs, ft, _) = theme.frame_size;
            // The middle of the middle cell, compared with the middle of the
            // top-middle cell, which is empty in both.
            let s = 6;
            let mid = |cx: usize, cy: usize| gone.glyphs[((ft + 1 + cy * s + s / 2) / 2) * gone.width + fs + 1 + cx * s + s / 2];
            assert_eq!(mid(1, 1), mid(1, 0), "{}", theme.name);
        }
    }

    #[test]
    fn slides_follow_the_path_and_end_in_place() {
        let path = [Dir::Down, Dir::Right, Dir::Right];
        assert_eq!(slide_offset(&path, 6, 0.0), (-12, -6));
        assert_eq!(slide_offset(&path, 6, 1.0), (0, 0));
        // Halfway through a three-step drag the block is half-way along
        // the middle step.
        assert_eq!(slide_offset(&path, 6, 0.5), (-9, 0));
        let mut last = slide_offset(&path, 6, 0.0);
        for i in 1..=20 {
            let now = slide_offset(&path, 6, i as f32 / 20.0);
            assert!((now.0 - last.0).abs() + (now.1 - last.1).abs() <= 6);
            last = now;
        }
        assert!(slide_millis(1) < slide_millis(3));
        assert_eq!(slide_millis(50), 420);
    }

    #[test]
    fn exit_fades_from_full_to_nothing() {
        assert_eq!(exit_opacity(0.0), 1.0);
        assert_eq!(exit_opacity(0.2), 1.0);
        assert_eq!(exit_opacity(1.0), 0.0);
        assert!((0.0..1.0).contains(&exit_opacity(0.6)));
        let unsolved = game("A..\n...", ('A', 2, 1));
        assert_eq!(exiting_block(&unsolved), None);
        let solved = game("A..\n...", ('A', 0, 0));
        assert_eq!(exiting_block(&solved), Some(0));
    }

    #[test]
    fn exit_side_is_marked_in_the_frame() {
        let down = game("A..\n...\n...", ('A', 1, 2));
        let right = game("A..\n...\n...", ('A', 2, 1));
        assert_eq!(down.puzzle().exit, Some(Dir::Down));
        assert_eq!(right.puzzle().exit, Some(Dir::Right));
        let theme = &THEMES[0];
        let options = PaintOptions::default();
        let (d, r) = (paint(&down, theme, &options), paint(&right, theme, &options));
        let last_row = |c: &Canvas| c.glyphs[(c.height - 1) * c.width..].to_vec();
        assert_ne!(last_row(&d), last_row(&r));
    }
}
