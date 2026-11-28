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
cargo run --release              # start the game
cargo run --release -- play klotski
cargo run --release -- list
cargo run --release -- show klotski
cargo run --release -- solve klotski --alternatives 3
cargo run --release -- solve path/to/puzzle.toml --boards
```

`dd help` lists every command and option.

## Playing

- `s` puts a cursor on a block; the arrows move it between blocks.
- Shift+arrow slides the block under the cursor one cell. Space grabs it
  instead: the arrows then slide it, Space or Enter drops it, and Esc puts
  it back where the grab began.
- `j` shows every block's label; type one to jump the cursor there.
- `u` and `r` undo and redo whole moves; `g` shows or hides the goal.
- `h` gives a hint once the solver has finished (the side panel shows ✓):
  the block to move pulses and a faint copy shows where it should go. Hints
  count towards your attempt, so you are asked first unless you tick
  "don't ask again".
- Esc steps back a mode, `?` lists the keys for the mode you are in, and
  `.` opens the pause menu.

Progress saves when you leave a puzzle, and Continue on the home screen
returns you to it.

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
`dark`, `light`, `escape-dark`, `escape-light` and `liberation` (lacquered
blocks in a carved box). `--play` animates the shortest solution and the key
block's exit; `dd help` lists the other options.

Everything `dd` keeps lives in `.data/` here (git-ignored), or wherever
`$DD_HOME` points. Settings are in `.data/settings.toml`; `dd settings`
shows them and `dd settings init` writes the defaults:

```toml
theme = "escape-dark"
grain = 1                    # texture, 0 to 3, for textured themes; 2 is as designed
labels = "on-demand"         # or "always", "off"
show_goal = false            # outline where the key block has to go
grid = true                  # outline empty cells
scale = 6                    # characters per cell across
animations = true            # slide blocks and fade the key block out when solved
confirm_hints = true         # ask before giving a hint
show_solver_length = false   # show the shortest solution's length while playing
mark_reachable = false       # mark where a grabbed block can go
best = "overall"             # or "no-hints-first"
# data_dir = "../somewhere"  # where puzzles, progress and records live
```

`dd settings move-data <folder>` copies your puzzles, progress and records
to another folder and points `data_dir` at it. It never deletes anything:
files already in the new folder are left as they are, and the old copy
stays for you to remove.

## Counting moves

A move is one drag: a block taken along any path while everything else stays
still. Consecutive steps of the same block count once, and a block put back
where its move began counts as no move. The solver minimises this count by
default; `--steps` minimises single-cell steps instead.

## Tests

```bash
cargo test
```
