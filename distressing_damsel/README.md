# Distressing Damsel

A general sliding-block puzzle engine and solver, started in order to solve
the three escape puzzles:
- Maiden's Escape
- Princess' Escape
- Queen's Escape

It handles any puzzle of the same family: rectangular or irregular blocks,
walls, blocks that only slide along one axis, and goals that involve one
block or several.

## Layout

| Crate     | What it does                                                        |
| --------- | ------------------------------------------------------------------- |
| `engine/` | Rules, positions, move generation, validation, codes, puzzle files  |
| `solver/` | Breadth-first search for the shortest solution, alternatives, hints |
| `tui/`    | The `dd` program. Headless commands for now; the full TUI is next   |
| `puzzles/`| Example puzzle files, built into `dd`                               |

The engine and solver do no I/O and start no threads of their own, so they
can also be compiled for the browser. The solver is driven in steps
(`Solver::step(budget)`); `dd_solver::background` wraps that in a thread for
local use.

## Usage

```bash
cargo run --release -- list
cargo run --release -- show klotski
cargo run --release -- solve klotski --alternatives 3
cargo run --release -- solve path/to/puzzle.toml --boards
```

`dd help` lists every command and option.

## Puzzle files

```toml
name = "Example"
grid = """
ABBC
ABBC
D..E
"""

[goal]
block = "B"
at = [1, 1]
```

- Each letter or digit is one block; cells sharing a character form the
  block, so any connected shape can be drawn. `.` is empty and `#` is a wall.
- `at` is `[column, row]`, counted from zero at the top left, for the
  top-left corner of the block's bounding box.
- Use several `[[goal]]` tables when more than one block has a target.
- Blocks of the same shape are interchangeable to the solver. A block named
  in a goal is kept distinct, unless you group blocks yourself:

```toml
[blocks.PQ]          # applies to blocks P and Q
tag = "red"          # either may satisfy a goal set for the other
axis = "horizontal"  # or "vertical"; omit for free movement
```

Boards are limited to 64 cells in this version.

A puzzle with a single target can name the side the key block leaves by once
solved, with a top-level `exit = "down"` (or `up`, `left`, `right`). Without
it, the exit is the edge the target touches (preferring down, right, up,
left), and a target away from every edge has no exit.

## Looks and settings

`dd preview <puzzle>` draws a puzzle in 24-bit colour in each theme:
`dark`, `light`, `escape-dark`, `escape-light` and `heirloom` (lacquered
blocks in a carved box). `--exit` solves the puzzle and plays the key block's
exit; `dd help` lists the other options.

Settings live in `~/.config/dd/settings.toml` (or `$XDG_CONFIG_HOME/dd/`,
or wherever `$DD_CONFIG` points). `dd settings` shows them and
`dd settings init` writes the defaults:

```toml
theme = "escape-dark"
grain = 1.0           # texture: 0 smooth, 1 the theme's own, up to 2
labels = "on-demand"  # or "always", "off"
show_goal = false     # outline where the key block has to go
scale = 6             # characters per cell across
animations = true     # play the exit when a puzzle is solved
```

## Counting moves

A move is one drag: a block taken along any path while everything else stays
still. Consecutive steps of the same block count once, and a block put back
where its move began counts as no move. The solver minimises this count by
default; `--steps` minimises single-cell steps instead.

## Tests

```bash
cargo test
```
