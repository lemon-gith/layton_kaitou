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

## Counting moves

A move is one drag: a block taken along any path while everything else stays
still. Consecutive steps of the same block count once, and a block put back
where its move began counts as no move. The solver minimises this count by
default; `--steps` minimises single-cell steps instead.

## Tests

```bash
cargo test
```
