# Pesticide — Pest TUI for Laravel

## Overview

Pesticide is a Rust-based terminal user interface (TUI) for running [Pest PHP](https://pestphp.com/) tests in Laravel projects. It provides an interactive tree view of the test suite (inspired by [neotest](https://github.com/nvim-neotest/neotest)), the ability to run individual tests/files/folders, inline coverage reporting with source-level drill-down, watch mode, and parallel execution by default.

## Architecture

```
┌──────────────────────────────────────────────┐
│                  pesticide                    │
│                                              │
│  ┌──────────┐  ┌───────────┐  ┌──────────┐  │
│  │ Discovery │  │  Runner   │  │   TUI    │  │
│  │  Module   │  │  Module   │  │  Module  │  │
│  └────┬─────┘  └─────┬─────┘  └────┬─────┘  │
│       │              │              │        │
│       ▼              ▼              ▼        │
│  ┌──────────────────────────────────────┐    │
│  │           Pest CLI (subprocess)       │    │
│  └──────────────────────────────────────┘    │
│       │              │              │        │
│  ┌──────────────────────────────────────┐    │
│  │        File Watcher (notify crate)    │    │
│  └──────────────────────────────────────┘    │
└──────────────────────────────────────────────┘
```

### Core Modules

- **Discovery** — shells out to `./vendor/bin/pest --list-tests` to build the test tree. Re-discovers on watch events.
- **Runner** — spawns `./vendor/bin/pest` subprocesses with appropriate flags (`--parallel`, `--coverage`, `--filter`). Streams output in real-time. Parses results via `--log-junit` or `--teamcity` for structured result mapping.
- **TUI** — Ratatui-based UI with single-pane layout (tree top, output bottom, keybindings footer).
- **Watcher** — Uses the `notify` crate to watch `tests/`, `app/`, and `app-modules/*/tests/` for changes. Triggers re-run of affected tests.

### Key Crates

- `ratatui` + `crossterm` — TUI rendering
- `tokio` — async runtime for subprocess management and file watching
- `notify` — filesystem watcher
- `roxmltree` or `quick-xml` — parse Clover coverage XML
- `clap` — CLI argument parsing
- `serde` / `serde_json` — data serialization

## TUI Layout

Single-pane tree with bottom output panel:

```
┌─ Pesticide ─ ~/Repos/quantiforme ──── ●  watching ─┐
│ ▼ tests/ (749 tests)                    92% coverage │
│   ▼ Feature/                                   88%  │
│     ▼ Auth/                                    95%  │
│       ✓ LoginTest.php (3 tests)                100% │
│         ✓ it can login with valid credentials        │
│         ✗ it rejects invalid password                │
│         ✓ it throttles login attempts                │
│       ◌ RegisterTest.php (5 tests)              90% │
│     ▶ Players/ (12 tests)                       85% │
│   ▼ Unit/                                      98%  │
│     ▶ Models/ (15 tests)                       100% │
├──────────────────────────────────────────────────────┤
│ FAIL  it rejects invalid password                    │
│   Expected status 401                                │
│   Received status 200                                │
│   at tests/Feature/Auth/LoginTest.php:24             │
├──────────────────────────────────────────────────────┤
│ ↑↓ navigate  ←→ fold  enter run  a all  c coverage  │
│ w watch  p parallel  f filter  q quit                │
└──────────────────────────────────────────────────────┘
```

### Tree Node States

- `✓` — passed (green)
- `✗` — failed (red)
- `◌` — not yet run (dim)
- `⟳` — currently running (yellow, animated spinner)
- `▶` / `▼` — collapsed / expanded folder

### Keybindings

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up in tree |
| `↓` / `j` | Move down in tree |
| `←` / `h` | Collapse folder / go to parent |
| `→` / `l` | Expand folder |
| `Enter` | Run selected test/file/folder |
| `a` | Run all tests |
| `c` | Toggle coverage mode |
| `w` | Toggle watch mode |
| `p` | Toggle parallel (on by default) |
| `f` | Open filter/search prompt |
| `q` | Quit |
| `Tab` | Switch focus between tree and output |
| `G` | Scroll to bottom of output |
| `g` | Scroll to top of output |

### Output Panel

- Shows output for the currently selected test/file
- Streams output in real-time during runs
- Failed tests show error message and file location
- Scrollable independently when focused via `Tab`

## Coverage

### Coverage Table View

Pressing `c` toggles into coverage mode, replacing the tree with a sortable table:

```
┌─ Pesticide ─ Coverage ──────────────── 87.3% total ─┐
│ File                          Lines  Hit  Miss    %  │
│ ─────────────────────────────────────────────────── │
│ app/Models/Player.php           142   138    4  97%  │
│ app/Models/Injury.php            89    82    7  92%  │
│ app/Services/AuthService.php    234   198   36  85%  │
│ app/Http/Controllers/Login…     187   140   47  75%  │
│ app/Services/TenantService…      96    42   54  44%  │
├──────────────────────────────────────────────────────┤
│ ↑↓ navigate  enter drill-in  s sort  c back to tree │
│ t threshold  q quit                                  │
└──────────────────────────────────────────────────────┘
```

- `s` cycles sort: by percentage, by misses, by filename
- `t` sets a coverage threshold — files below it are highlighted red

### Coverage Source View

Pressing `Enter` on a file in the coverage table shows source with line-level highlighting:

```
┌─ Pesticide ─ app/Services/TenantService.php ── 44% ─┐
│  40 │     public function resolve(Request $req)      │
│  41 │     {                                          │
│  42 │ ██      $domain = $req->getHost();             │
│  43 │ ██      $tenant = Tenant::where('domain',      │
│  44 │ ██          $domain)->first();                 │
│  45 │                                                │
│  46 │ ░░      if (!$tenant) {                        │
│  47 │ ░░          throw new TenantNotFound($domain); │
│  48 │ ░░      }                                      │
│  49 │                                                │
│  50 │ ██      $this->setConnection($tenant);         │
│  51 │ ██      return $tenant;                        │
├──────────────────────────────────────────────────────┤
│ ↑↓ scroll  esc back to table  n next uncovered       │
└──────────────────────────────────────────────────────┘
```

- `██` green background = covered line
- `░░` red background = uncovered line
- Plain = non-executable line
- `n` jumps to next uncovered block
- `Esc` goes back one level (source → table → tree)

### Data Source

Coverage data from Pest's `--coverage-clover` flag, which outputs Clover XML with line-level hit data.

## Test Discovery & Running

### Discovery

- On startup, run `./vendor/bin/pest --list-tests` and parse output to build tree
- Auto-detect project root by looking for `vendor/bin/pest` from cwd upward
- Re-discover when watch mode detects new/deleted test files

### Running Tests

| Action | Command |
|--------|---------|
| Single test | `pest --filter="test name" --parallel` |
| Single file | `pest tests/Feature/Auth/LoginTest.php --parallel` |
| Folder | `pest tests/Feature/Auth/ --parallel` |
| All tests | `pest --parallel` |
| With coverage | Append `--coverage-clover=.pesticide/coverage.xml` |

### Parallel by Default

- `--parallel` flag always included unless toggled off with `p`
- Status bar shows current mode: `∥ parallel` or `→ sequential`

### Output Parsing

- Stream stdout/stderr in real-time to the output panel
- Parse exit code: 0 = all passed, 1 = failures, 2 = error
- Use `--teamcity` or `--log-junit` format for structured result parsing to map each test result back to its tree node

### Process Management

- Tests run in a background `tokio` task
- Starting a new run kills any existing process first
- `Ctrl+C` cancels the current run, not the TUI

## Watch Mode

- Toggle with `w` — header shows `● watching` (green dot) when active
- Watches: `tests/`, `app/`, `app-modules/*/tests/`
- Debounce: 500ms after last file change
- Ignores: `.git/`, `vendor/`, `node_modules/`, `.pesticide/`
- Test file changes → re-run that specific test file
- Source file changes → re-run all tests (no smart source-to-test mapping for v1)
- New/deleted test files → re-discover tree, then run
- Manual runs take priority over watch-triggered runs

## Distribution

Standalone binary installed via `cargo install pesticide` or Homebrew. Run `pesticide` from any Laravel project root.

## Project Structure

```
pesticide/
├── Cargo.toml
├── src/
│   ├── main.rs                # Entry point, arg parsing, app init
│   ├── app.rs                 # App state, event loop
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── tree.rs            # Tree widget rendering
│   │   ├── output.rs          # Output panel rendering
│   │   ├── coverage_table.rs  # Coverage summary table
│   │   ├── coverage_source.rs # Source code with coverage highlighting
│   │   └── footer.rs          # Keybindings bar
│   ├── pest/
│   │   ├── mod.rs
│   │   ├── discovery.rs       # --list-tests parsing → tree
│   │   ├── runner.rs          # Spawn pest, stream output
│   │   └── coverage.rs        # Parse clover XML
│   ├── tree/
│   │   ├── mod.rs
│   │   └── node.rs            # Tree data structure (folders, files, tests)
│   └── watcher.rs             # File system watcher
└── .pesticide/                # Runtime dir (in target project, gitignored)
    └── coverage.xml
```
