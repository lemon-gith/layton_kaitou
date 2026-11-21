//! Puzzles built into `dd`: (name, file contents).

pub const BUILT_IN: &[(&str, &str)] = &[
    ("first_steps", include_str!("../../puzzles/first_steps.toml")),
    ("klotski", include_str!("../../puzzles/klotski.toml")),
    ("gridlock", include_str!("../../puzzles/gridlock.toml")),
    ("odd_shapes", include_str!("../../puzzles/odd_shapes.toml")),
    ("trading_places", include_str!("../../puzzles/trading_places.toml")),
];
