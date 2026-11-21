//! Paints a game onto a [`Canvas`], and the animation of a solved puzzle's
//! key block leaving the box.

use std::collections::HashMap;

use dd_engine::{bit, cells, Dir, Game, Mask};

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
    /// Outline where the key block has to go.
    pub show_goal: bool,
    /// Multiplies the theme's texture: 0 is smooth, 1 the theme's own.
    pub grain: f32,
}

impl Default for PaintOptions {
    fn default() -> Self {
        PaintOptions {
            scale: 6,
            labels: HashMap::new(),
            selected: None,
            show_goal: false,
            grain: 1.0,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Owner {
    Backdrop,
    Frame,
    /// The gap in the frame the key block leaves by.
    Door,
    Floor,
    Wall,
    Face(usize),
    /// The ledge outside the door.
    Platform,
    /// The raised edge around the ledge.
    Rail,
}

impl Owner {
    fn raised(self) -> bool {
        matches!(self, Owner::Frame | Owner::Wall | Owner::Face(_) | Owner::Rail)
    }
}

/// Where the door is and how far a leaving block travels.
#[derive(Copy, Clone, Debug)]
struct Exit {
    dir: Dir,
    /// Span of the door across the exit direction, in image coordinates.
    lo: usize,
    hi: usize,
    /// Length of the ledge, including its end rail.
    ledge: usize,
    /// Image coordinate the leaving block's leading face edge stops at:
    /// a row for up and down, a column for left and right.
    stop: isize,
}

/// A painted board before it is packed into characters.
struct Image {
    width: usize,
    height: usize,
    pixels: Vec<Rgb>,
    owner: Vec<Owner>,
    exit: Option<Exit>,
}

/// The block the goal is about, if the puzzle has a single target and that
/// block is on it.
fn key_block(game: &Game) -> Option<usize> {
    let targets = game.layout().goal().targets();
    let &[(class, anchor)] = targets.as_slice() else {
        return None;
    };
    (0..game.block_count())
        .find(|&b| game.puzzle().blocks[b].class == class && game.anchor(b) == Some(anchor))
}

fn paint_image(game: &Game, theme: &Theme, options: &PaintOptions, hidden: Option<usize>) -> Image {
    let layout = game.layout();
    let s = options.scale.max(4) & !1;
    let (w, h) = (layout.width() as usize, layout.height() as usize);
    let (fs, ft, fb) = theme.frame_size;
    let bw = fs + 1 + w * s + fs;
    let bh = ft + 1 + h * s + fb;
    let g = theme.grain * options.grain.max(0.0);

    let targets = layout.goal().targets();
    let goal_cells: Mask = targets
        .iter()
        .fold(0, |acc, &(class, anchor)| acc | layout.block_mask(class, anchor));

    // The door and ledge, sized to the key block.
    let mut extension = 0;
    let mut exit = game.puzzle().exit.zip(targets.first()).map(|(dir, &(class, anchor))| {
        let xy: Vec<(usize, usize)> = cells(layout.block_mask(class, anchor))
            .map(|c| {
                let (x, y) = layout.xy(c);
                (x as usize, y as usize)
            })
            .collect();
        let min_x = xy.iter().map(|c| c.0).min().unwrap_or(0);
        let max_x = xy.iter().map(|c| c.0).max().unwrap_or(0);
        let min_y = xy.iter().map(|c| c.1).min().unwrap_or(0);
        let max_y = xy.iter().map(|c| c.1).max().unwrap_or(0);
        let vertical = matches!(dir, Dir::Up | Dir::Down);
        let on_edge: Vec<usize> = xy
            .iter()
            .filter(|&&(x, y)| match dir {
                Dir::Down => y == h - 1,
                Dir::Up => y == 0,
                Dir::Right => x == w - 1,
                Dir::Left => x == 0,
            })
            .map(|&(x, y)| if vertical { x } else { y })
            .collect();
        let first = on_edge.iter().copied().min().unwrap_or(0);
        let last = on_edge.iter().copied().max().unwrap_or(0);
        let depth = if vertical { (max_y - min_y + 1) * s - 1 } else { (max_x - min_x + 1) * s - 1 };
        // A floor margin each side of the block, then the end rail.
        let ledge = depth + 3;
        extension = ledge + 1;
        if vertical && extension % 2 == 1 {
            extension += 1;
        }
        let origin = if vertical { fs + 1 } else { ft + 1 };
        Exit { dir, lo: origin + first * s, hi: origin + last * s + s - 1, ledge, stop: 0 }
    });
    let (ox, oy, iw, ih) = match exit.map(|e| e.dir) {
        None => (0, 0, bw, bh),
        Some(Dir::Down) => (0, 0, bw, bh + extension),
        Some(Dir::Up) => (0, extension, bw, bh + extension),
        Some(Dir::Right) => (0, 0, bw + extension, bh),
        Some(Dir::Left) => (extension, 0, bw + extension, bh),
    };
    if let Some(e) = exit.as_mut() {
        if matches!(e.dir, Dir::Up | Dir::Down) {
            e.lo += ox;
            e.hi += ox;
        } else {
            e.lo += oy;
            e.hi += oy;
        }
        e.stop = match e.dir {
            Dir::Down => (oy + bh + 1) as isize,
            Dir::Up => oy as isize - 2,
            Dir::Right => (ox + bw + 1) as isize,
            Dir::Left => ox as isize - 2,
        };
    }
    let (left, top) = (ox + fs + 1, oy + ft + 1);

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
    let mut owner = vec![Owner::Backdrop; iw * ih];
    for y in 0..ih {
        for x in 0..iw {
            let o = &mut owner[y * iw + x];
            let inside = x >= ox && x < ox + bw && y >= oy && y < oy + bh;
            if !inside {
                if let Some(e) = exit {
                    let (d, p) = match e.dir {
                        Dir::Down => (y as isize - (oy + bh) as isize, x),
                        Dir::Up => (oy as isize - 1 - y as isize, x),
                        Dir::Right => (x as isize - (ox + bw) as isize, y),
                        Dir::Left => (ox as isize - 1 - x as isize, y),
                    };
                    let d = d as usize;
                    if d < e.ledge && p + 1 >= e.lo && p <= e.hi {
                        *o = if p >= e.lo && p < e.hi && d < e.ledge - 1 { Owner::Platform } else { Owner::Rail };
                    }
                }
                continue;
            }
            let (bx0, by0) = (x - ox, y - oy);
            if bx0 < fs || bx0 >= bw - fs || by0 < ft || by0 >= bh - fb {
                *o = Owner::Frame;
                if let Some(e) = exit {
                    let on_side = match e.dir {
                        Dir::Down => by0 >= bh - fb,
                        Dir::Up => by0 < ft,
                        Dir::Right => bx0 >= bw - fs,
                        Dir::Left => bx0 < fs,
                    };
                    let p = if matches!(e.dir, Dir::Up | Dir::Down) { x } else { y };
                    if on_side && p >= e.lo && p < e.hi {
                        *o = Owner::Door;
                    }
                }
                continue;
            }
            *o = Owner::Floor;
            // One pixel of floor before the first cell matches the gap every
            // cell leaves after itself.
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
        if x < 0 || y < 0 || x >= iw as isize || y >= ih as isize {
            Owner::Backdrop
        } else {
            owner[y as usize * iw + x as usize]
        }
    };

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

    let key_classes: Vec<u8> = targets.iter().map(|t| t.0).collect();
    let single_key = key_classes.len() == 1;
    let colour_of_block = |block: usize| -> Rgb {
        let class = game.puzzle().blocks[block].class;
        if single_key && key_classes[0] == class {
            return theme.key;
        }
        let earlier = (0..class).filter(|c| !single_key || *c != key_classes[0]).count();
        theme.blocks[earlier % theme.blocks.len()]
    };

    // Each block face's bounding box, for gloss.
    let mut boxes: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new();
    if theme.gloss > 0.0 {
        for y in 0..ih {
            for x in 0..iw {
                if let Owner::Face(b) = owner[y * iw + x] {
                    let e = boxes.entry(b).or_insert((x, y, x, y));
                    *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
                }
            }
        }
    }

    let floor_colour = |x: usize, y: usize, grid: bool| -> Rgb {
        let (xi, yi) = (x as isize, y as isize);
        let mut c = theme.floor.shade(grain(theme.floor_grain, x as i32, y as i32, 7) * 0.25 * g);
        if grid {
            let (bx, by) = (xi - left as isize, yi - top as isize);
            if bx < 0 || by < 0 || bx as usize % s == s - 1 || by as usize % s == s - 1 {
                c = c.shade(if theme.dark { theme.floor_grid } else { -theme.floor_grid });
            }
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
    };

    let mut pixels = vec![theme.backdrop; iw * ih];
    for y in 0..ih {
        for x in 0..iw {
            let (xi, yi) = (x as isize, y as isize);
            let here = owner[y * iw + x];
            pixels[y * iw + x] = match here {
                Owner::Backdrop => theme.backdrop,
                Owner::Frame => {
                    let (bx0, by0) = (x - ox, y - oy);
                    // Mitred frame: grain follows the nearest outer edge.
                    let d = [bx0, bw - 1 - bx0, by0, bh - 1 - by0];
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
                Owner::Rail => {
                    let along = if matches!(exit.map(|e| e.dir), Some(Dir::Up | Dir::Down)) { y } else { x };
                    let mut c = theme.frame.shade(grain(theme.frame_grain, along as i32, x as i32 ^ y as i32, 5) * 0.22 * g);
                    if !at(xi, yi - 1).raised() || !at(xi - 1, yi).raised() {
                        c = c.shade(theme.bevel * 0.6);
                    } else if !at(xi, yi + 1).raised() || !at(xi + 1, yi).raised() {
                        c = c.shade(-theme.bevel * 0.8);
                    }
                    c
                }
                Owner::Door | Owner::Platform => floor_colour(x, y, false),
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
                    let mut c = floor_colour(x, y, true);
                    if options.show_goal && goal_face(xi, yi) {
                        let edge = [(0, -1), (0, 1), (-1, 0), (1, 0)]
                            .iter()
                            .any(|&(dx, dy)| !goal_face(xi + dx, yi + dy));
                        c = c.mix(theme.goal, if edge { 0.7 } else { 0.12 });
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
                    if let Some(&(x0, y0, _, y1)) = boxes.get(&block) {
                        c = gloss(c, theme.gloss, x - x0, y - y0, y1 - y0 + 1);
                    }
                    c
                }
            };
        }
    }

    // Selection: a ring just outside the block.
    if let Some(sel) = options.selected.filter(|&b| Some(b) != hidden) {
        let face = |x: isize, y: isize| at(x, y) == Owner::Face(sel);
        let mut ring = Vec::new();
        for y in 0..ih as isize {
            for x in 0..iw as isize {
                if !face(x, y) && (-1..=1).any(|dy| (-1..=1).any(|dx| face(x + dx, y + dy))) {
                    ring.push(y as usize * iw + x as usize);
                }
            }
        }
        for i in ring {
            pixels[i] = theme.accent;
        }
    }

    Image { width: iw, height: ih, pixels, owner, exit }
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

/// Packs pixels into characters and writes labels on the blocks that own
/// the pixels in `owner`.
fn to_canvas(image: &Image, pixels: &[Rgb], theme: &Theme, labels: &HashMap<usize, String>) -> Canvas {
    let (pw, cw, ch) = (image.width, image.width, image.height / 2);
    let mut glyphs = Vec::with_capacity(cw * ch);
    for row in 0..ch {
        for x in 0..cw {
            let (upper, lower) = (pixels[row * 2 * pw + x], pixels[(row * 2 + 1) * pw + x]);
            glyphs.push(if upper == lower {
                Glyph { ch: ' ', fg: upper, bg: upper, bold: false }
            } else {
                Glyph { ch: '▀', fg: upper, bg: lower, bold: false }
            });
        }
    }

    // Each label goes on the run of whole characters nearest the face's
    // centre of mass. That point can be off the block (an L's corner), so
    // only runs entirely on the face are considered.
    for (&block, text) in labels {
        let len = text.chars().count();
        if len == 0 {
            continue;
        }
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
        if n == 0.0 {
            continue;
        }
        let (mx, my) = (sx / n, sy / n);
        let mut best: Option<(f32, usize, usize)> = None;
        for row in 0..ch {
            for x in 0..cw.saturating_sub(len - 1) {
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
        if let Some((_, x, row)) = best {
            for (i, c) in text.chars().enumerate() {
                let bg = pixels[row * 2 * pw + x + i].mix(pixels[(row * 2 + 1) * pw + x + i], 0.5);
                let fg = if bg.luma() > 0.62 { theme.text_on_light } else { theme.text };
                glyphs[row * cw + x + i] = Glyph { ch: c, fg, bg, bold: true };
            }
        }
    }

    Canvas { width: cw, height: ch, glyphs }
}

pub fn paint(game: &Game, theme: &Theme, options: &PaintOptions) -> Canvas {
    let image = paint_image(game, theme, options, None);
    to_canvas(&image, &image.pixels, theme, &options.labels)
}

/// Frames of the key block leaving a solved puzzle: it slides out through
/// the door onto the ledge and fades away. Puzzles with no door get a glint
/// and a fade where the block stands. The last frame is the box without the
/// block; every frame is the same size as [`paint`]'s canvas.
///
/// Returns just the still board if the puzzle is not solved or has several
/// targets.
pub fn exit_frames(game: &Game, theme: &Theme, options: &PaintOptions) -> Vec<Canvas> {
    let options = PaintOptions { selected: None, ..options.clone() };
    let Some(key) = key_block(game).filter(|_| game.is_solved()) else {
        return vec![paint(game, theme, &options)];
    };
    let full = paint_image(game, theme, &options, None);
    let empty = paint_image(game, theme, &options, Some(key));
    let mut labels = options.labels.clone();
    labels.remove(&key);

    let block: Vec<(isize, isize, Rgb)> = (0..full.height)
        .flat_map(|y| (0..full.width).map(move |x| (x, y)))
        .filter(|&(x, y)| full.owner[y * full.width + x] == Owner::Face(key))
        .map(|(x, y)| (x as isize, y as isize, full.pixels[y * full.width + x]))
        .collect();
    let frame = |offset: (isize, isize), fade: f32, glint: f32| -> Canvas {
        let mut pixels = empty.pixels.clone();
        for &(x, y, c) in &block {
            let (nx, ny) = (x + offset.0, y + offset.1);
            if nx < 0 || ny < 0 || nx >= empty.width as isize || ny >= empty.height as isize {
                continue;
            }
            let i = ny as usize * empty.width + nx as usize;
            pixels[i] = c.shade(glint).mix(pixels[i], fade);
        }
        to_canvas(&empty, &pixels, theme, &labels)
    };

    // The key block's label goes for the whole animation, so it does not
    // vanish one frame in.
    let mut frames = vec![to_canvas(&full, &full.pixels, theme, &labels)];
    let fades = [0.15, 0.3, 0.5, 0.7, 0.85];
    match empty.exit {
        Some(exit) => {
            let ys = block.iter().map(|b| b.1);
            let xs = block.iter().map(|b| b.0);
            let travel = match exit.dir {
                Dir::Down => exit.stop - ys.min().unwrap_or(0),
                Dir::Up => ys.max().unwrap_or(0) - exit.stop,
                Dir::Right => exit.stop - xs.min().unwrap_or(0),
                Dir::Left => xs.max().unwrap_or(0) - exit.stop,
            };
            let (dx, dy) = exit.dir.delta();
            let steps = travel.max(8) as usize;
            let mut last = 0;
            for i in 1..=steps {
                // Ease in and out.
                let t = i as f32 / steps as f32;
                let e = t * t * (3.0 - 2.0 * t);
                let along = (e * travel as f32).round() as isize;
                if along != last {
                    frames.push(frame((dx as isize * along, dy as isize * along), 0.0, 0.0));
                    last = along;
                }
            }
            let offset = (dx as isize * travel, dy as isize * travel);
            for f in fades {
                frames.push(frame(offset, f, 0.0));
            }
        }
        None => {
            for glint in [0.15, 0.3, 0.15] {
                frames.push(frame((0, 0), 0.0, glint));
            }
            for f in fades {
                frames.push(frame((0, 0), f, 0.0));
            }
        }
    }
    frames.push(to_canvas(&empty, &empty.pixels, theme, &labels));
    frames
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
        for (_, source) in crate::BUILT_IN {
            let (puzzle, _) = dd_engine::file::parse(source).unwrap().build().unwrap();
            let game = Game::new(puzzle);
            let labels: HashMap<usize, String> = (0..game.block_count()).map(|b| (b, format!("{b}"))).collect();
            for theme in &THEMES {
                for scale in [4, 6, 8, 12] {
                    for show_goal in [false, true] {
                        let options = PaintOptions {
                            scale,
                            labels: labels.clone(),
                            selected: Some(0),
                            show_goal,
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
    fn goal_outline_is_optional() {
        let game = game("A..\n...", ('A', 2, 1));
        let theme = &THEMES[0];
        let plain = paint(&game, theme, &PaintOptions::default());
        let shown = paint(&game, theme, &PaintOptions { show_goal: true, ..Default::default() });
        assert_ne!(plain, shown);
    }

    #[test]
    fn grain_setting_scales_texture() {
        let game = game("A..\n...", ('A', 2, 1));
        let theme = &THEMES[2];
        let smooth = paint(&game, theme, &PaintOptions { grain: 0.0, ..Default::default() });
        let normal = paint(&game, theme, &PaintOptions::default());
        assert_ne!(smooth, normal);
        // With no grain, the middle of an empty cell is one flat colour.
        let distinct: std::collections::HashSet<_> = smooth.glyphs.iter().map(|g| g.bg).collect();
        let textured: std::collections::HashSet<_> = normal.glyphs.iter().map(|g| g.bg).collect();
        assert!(distinct.len() < textured.len());
    }

    #[test]
    fn exit_animation_slides_the_key_block_out() {
        for (grid, goal, dir) in [
            ("A..\n...\n...", ('A', 2, 1), Dir::Right),
            ("..A\n...\n...", ('A', 0, 1), Dir::Left),
            ("A..\n...\n...", ('A', 1, 2), Dir::Down),
            ("...\n...\n..A", ('A', 1, 0), Dir::Up),
        ] {
            let mut game = game(grid, goal);
            assert_eq!(game.puzzle().exit, Some(dir));
            let still = paint(&game, &THEMES[2], &PaintOptions::default());
            // Not solved yet: no animation.
            assert_eq!(exit_frames(&game, &THEMES[2], &PaintOptions::default()), vec![still.clone()]);

            let a = game.block_at(game.puzzle().blocks[0].start).unwrap();
            game.drag_to(a, game.layout().goal().targets()[0].1).unwrap();
            assert!(game.is_solved());
            let frames = exit_frames(&game, &THEMES[2], &PaintOptions::default());
            assert!(frames.len() > 10, "{dir:?}: {} frames", frames.len());
            assert!(frames.iter().all(|f| f.width == still.width && f.height == still.height));
            assert_eq!(frames[0], paint(&game, &THEMES[2], &PaintOptions::default()));
            assert!(frames.windows(2).all(|w| w[0] != w[1]));
        }
    }

    #[test]
    fn puzzles_without_a_door_fade_in_place() {
        // The target is in the middle of the board, so there is no way out;
        // the block already sits on it.
        let game = game("...\n.A.\n...", ('A', 1, 1));
        assert_eq!(game.puzzle().exit, None);
        assert!(game.is_solved());
        let frames = exit_frames(&game, &THEMES[4], &PaintOptions::default());
        assert!(frames.len() > 5);
        assert_eq!(frames.last().unwrap().width, frames[0].width);
    }
}
