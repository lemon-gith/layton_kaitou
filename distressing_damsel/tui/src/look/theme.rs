//! Colour themes.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(v: u32) -> Rgb {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Linear blend: `t = 0` is `self`, `t = 1` is `other`.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(f(self.0, other.0), f(self.1, other.1), f(self.2, other.2))
    }

    /// Positive amounts lighten towards white, negative darken towards black.
    pub fn shade(self, amount: f32) -> Rgb {
        if amount >= 0.0 {
            self.mix(Rgb(255, 255, 255), amount)
        } else {
            self.mix(Rgb(0, 0, 0), -amount)
        }
    }

    /// Perceived brightness, 0 to 1.
    pub fn luma(self) -> f32 {
        (0.299 * self.0 as f32 + 0.587 * self.1 as f32 + 0.114 * self.2 as f32) / 255.0
    }
}

/// Surface pattern.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Grain {
    Smooth,
    /// Fine speckle, like stone or card.
    Speckle,
    /// Long streaks along the piece.
    Wood,
    /// Courses of offset bricks.
    Brick,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    /// Behind and around the box.
    pub backdrop: Rgb,

    pub frame: Rgb,
    pub frame_grain: Grain,
    /// Frame thickness in pixels on the sides, top and bottom. Top plus
    /// bottom must be odd so the image comes out a whole number of rows.
    pub frame_size: (usize, usize, usize),
    /// Grooves, beading and studs cut into the frame.
    pub carved: bool,
    pub floor: Rgb,
    pub floor_grain: Grain,
    /// How strongly empty cells are outlined, 0 for not at all.
    pub floor_grid: f32,
    /// Shadow cast on the floor by raised things, 0 for none.
    pub shadow: f32,
    pub wall: Rgb,
    pub wall_grain: Grain,

    /// Colours for block classes, in order of first appearance.
    pub blocks: &'static [Rgb],
    /// Colour for the block that has to escape.
    pub key: Rgb,
    pub block_grain: Grain,
    /// Highlight and shadow on block edges, 0 for flat.
    pub bevel: f32,
    /// Lacquer: a sheen across the top of each block and a glint in its
    /// corner. 0 for none.
    pub gloss: f32,
    /// Strength of every grain at the default setting, 0 to 1.
    pub grain: f32,

    /// Goal outline, when shown.
    pub goal: Rgb,
    /// Selection outline.
    pub accent: Rgb,
    pub text: Rgb,
    pub text_on_light: Rgb,
}

/// Blue, green and purple first, then colours for bigger puzzles.
const FLAT_DARK: &[Rgb] = &[
    Rgb::hex(0x5b8fd9),
    Rgb::hex(0x55ad6e),
    Rgb::hex(0x9a7fd6),
    Rgb::hex(0xe0954a),
    Rgb::hex(0x8b929c),
    Rgb::hex(0xdcdfe4),
    Rgb::hex(0x47ad9b),
    Rgb::hex(0x5a606b),
];
const FLAT_LIGHT: &[Rgb] = &[
    Rgb::hex(0x6f9fe6),
    Rgb::hex(0x6cc283),
    Rgb::hex(0xb29ae8),
    Rgb::hex(0xf0a95e),
    Rgb::hex(0x9aa1ab),
    Rgb::hex(0xfbfbfc),
    Rgb::hex(0x5fbfac),
    Rgb::hex(0x5f6672),
];
const STONE_DARK: &[Rgb] = &[
    Rgb::hex(0x4d7ea6),
    Rgb::hex(0x4b9572),
    Rgb::hex(0x7b5ba3),
    Rgb::hex(0xc4853a),
    Rgb::hex(0x7d838c),
    Rgb::hex(0xd6cfbd),
    Rgb::hex(0x3d8c92),
    Rgb::hex(0x4a4f57),
];
const STONE_LIGHT: &[Rgb] = &[
    Rgb::hex(0x6a9cc9),
    Rgb::hex(0x68b48c),
    Rgb::hex(0x9b7cc4),
    Rgb::hex(0xdea15a),
    Rgb::hex(0x9aa0a8),
    Rgb::hex(0xf4efe3),
    Rgb::hex(0x5aaab0),
    Rgb::hex(0x5d636c),
];
const LACQUER: &[Rgb] = &[
    Rgb::hex(0x3a6fb8),
    Rgb::hex(0x35925a),
    Rgb::hex(0x7449ab),
    Rgb::hex(0xd6832a),
    Rgb::hex(0x858c96),
    Rgb::hex(0xe8dfca),
    Rgb::hex(0x24918f),
    Rgb::hex(0x393d45),
];

pub const THEMES: [Theme; 5] = [
    Theme {
        name: "dark",
        dark: true,
        backdrop: Rgb::hex(0x121418),
        frame: Rgb::hex(0x343a46),
        frame_grain: Grain::Smooth,
        frame_size: (4, 3, 4),
        carved: false,
        floor: Rgb::hex(0x1b1e24),
        floor_grain: Grain::Smooth,
        floor_grid: 0.18,
        shadow: 0.0,
        wall: Rgb::hex(0x343a46),
        wall_grain: Grain::Smooth,
        blocks: FLAT_DARK,
        key: Rgb::hex(0xe0525a),
        block_grain: Grain::Smooth,
        bevel: 0.0,
        gloss: 0.0,
        grain: 0.0,
        goal: Rgb::hex(0xe0525a),
        accent: Rgb::hex(0xf2f2f2),
        text: Rgb::hex(0xf4f4f4),
        text_on_light: Rgb::hex(0x14161b),
    },
    Theme {
        name: "light",
        dark: false,
        backdrop: Rgb::hex(0xe4e6ea),
        frame: Rgb::hex(0xb4bac5),
        frame_grain: Grain::Smooth,
        frame_size: (4, 3, 4),
        carved: false,
        floor: Rgb::hex(0xf1f2f5),
        floor_grain: Grain::Smooth,
        floor_grid: 0.07,
        shadow: 0.0,
        wall: Rgb::hex(0xb4bac5),
        wall_grain: Grain::Smooth,
        blocks: FLAT_LIGHT,
        key: Rgb::hex(0xe2545d),
        block_grain: Grain::Smooth,
        bevel: 0.0,
        gloss: 0.0,
        grain: 0.0,
        goal: Rgb::hex(0xd9434d),
        accent: Rgb::hex(0x1d2230),
        text: Rgb::hex(0xffffff),
        text_on_light: Rgb::hex(0x1d2230),
    },
    Theme {
        name: "escape-dark",
        dark: true,
        backdrop: Rgb::hex(0x17110d),
        frame: Rgb::hex(0x5c3b22),
        frame_grain: Grain::Wood,
        frame_size: (4, 3, 4),
        carved: false,
        floor: Rgb::hex(0x2b221c),
        floor_grain: Grain::Speckle,
        floor_grid: 0.12,
        shadow: 0.35,
        wall: Rgb::hex(0x4e3220),
        wall_grain: Grain::Wood,
        blocks: STONE_DARK,
        key: Rgb::hex(0xc0453a),
        block_grain: Grain::Speckle,
        bevel: 0.28,
        gloss: 0.0,
        grain: 1.0,
        goal: Rgb::hex(0xf0b648),
        accent: Rgb::hex(0xffe39a),
        text: Rgb::hex(0xfbf3e4),
        text_on_light: Rgb::hex(0x24170d),
    },
    Theme {
        name: "escape-light",
        dark: false,
        backdrop: Rgb::hex(0xefe6d6),
        frame: Rgb::hex(0xa8743f),
        frame_grain: Grain::Wood,
        frame_size: (4, 3, 4),
        carved: false,
        floor: Rgb::hex(0xdccaa6),
        floor_grain: Grain::Speckle,
        floor_grid: 0.07,
        shadow: 0.22,
        wall: Rgb::hex(0x9a6a3a),
        wall_grain: Grain::Wood,
        blocks: STONE_LIGHT,
        key: Rgb::hex(0xd4513f),
        block_grain: Grain::Speckle,
        bevel: 0.3,
        gloss: 0.0,
        grain: 0.85,
        goal: Rgb::hex(0xb8710f),
        accent: Rgb::hex(0x3a2210),
        text: Rgb::hex(0xfffaf0),
        text_on_light: Rgb::hex(0x2e1c0e),
    },
    Theme {
        name: "liberation",
        dark: true,
        backdrop: Rgb::hex(0x120c09),
        frame: Rgb::hex(0x6e3d20),
        frame_grain: Grain::Wood,
        frame_size: (6, 4, 5),
        carved: true,
        floor: Rgb::hex(0x2a1d15),
        floor_grain: Grain::Speckle,
        floor_grid: 0.1,
        shadow: 0.4,
        wall: Rgb::hex(0x5e3319),
        wall_grain: Grain::Wood,
        blocks: LACQUER,
        key: Rgb::hex(0xc8352e),
        block_grain: Grain::Speckle,
        bevel: 0.32,
        gloss: 0.34,
        grain: 0.75,
        goal: Rgb::hex(0xf2c14e),
        accent: Rgb::hex(0xffe7a3),
        text: Rgb::hex(0xfff8ec),
        text_on_light: Rgb::hex(0x24150b),
    },
];

pub fn by_name(name: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.name == name)
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263))
        .wrapping_add(seed.wrapping_mul(2_246_822_519));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 32_767.5 - 1.0
}

/// Pattern value in roughly -1 to 1 at a pixel. `along` runs with the grain.
pub fn grain(kind: Grain, along: i32, across: i32, seed: u32) -> f32 {
    match kind {
        Grain::Smooth => 0.0,
        Grain::Speckle => hash(along, across, seed) * 0.28,
        Grain::Wood => {
            let a = along as f32;
            let s = seed as f32;
            let warp = 0.9 * (a * 0.09 + s).sin() + 0.5 * (a * 0.031 + 2.3 * s).sin();
            let ring = ((across as f32 + warp) * 1.9 + s).sin();
            let knot = hash(along / 7, across, seed) * 0.25;
            ring * 0.55 + knot + hash(along, across, seed ^ 0x9e37) * 0.12
        }
        Grain::Brick => {
            let course = across.div_euclid(3);
            let offset = if course % 2 == 0 { 0 } else { 3 };
            if across.rem_euclid(3) == 0 || (along + offset).rem_euclid(6) == 0 {
                -0.9
            } else {
                hash(along / 6, course, seed) * 0.3 + hash(along, across, seed) * 0.1
            }
        }
    }
}
