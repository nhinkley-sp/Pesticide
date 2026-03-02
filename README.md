# Pesticide

A terminal UI for running [Pest PHP](https://pestphp.com/) tests in Laravel projects. Provides an interactive, neotest-like tree view with parallel execution, coverage analysis, watch mode, and filtering.

![Rust](https://img.shields.io/badge/rust-stable-orange) ![License](https://img.shields.io/badge/license-MIT-blue)

## Features

- **Test tree navigation** - Hierarchical view of your test suite (directories, files, tests) with expand/collapse
- **Flexible execution** - Run all tests, a directory, a single file, or an individual test
- **Parallel execution** - Enabled by default, leveraging Pest's `--parallel` flag
- **Coverage analysis** - Table and tree views with drill-down to line-level source coverage
- **Watch mode** - Auto-reruns tests when source or test files change (500ms debounce)
- **Filtering** - Interactive search to narrow down the test tree
- **Real-time output** - Streaming test output with scrollable panel
- **Vim-style navigation** - `j`/`k`, `h`/`l`, `g`/`G`, `Ctrl+U`/`Ctrl+D`

## Installation

### From source

```sh
cargo install --path .
```

### Prerequisites

- Rust toolchain (stable)
- A Laravel project with [Pest PHP](https://pestphp.com/) installed (`vendor/bin/pest`)

## Usage

```sh
# Run from your Laravel project root
pesticide

# Specify a project path
pesticide --path /path/to/laravel-project

# Run all tests immediately on launch
pesticide --run

# Run with coverage immediately
pesticide --coverage

# Start with watch mode enabled
pesticide --watch

# Disable parallel execution
pesticide --no-parallel

# Combine flags
pesticide --run --watch
```

## Keybindings

### Test Tree

| Key | Action |
|-----|--------|
| `j` / `k` / `↑` / `↓` | Navigate up/down |
| `h` / `l` / `←` / `→` | Collapse/expand node |
| `Enter` | Run selected scope |
| `a` | Run all tests |
| `c` | Run all with coverage |
| `w` | Toggle watch mode |
| `p` | Toggle parallel execution |
| `f` | Filter tests |
| `Tab` | Switch focus to output panel |
| `q` | Quit |

### Coverage Table

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate files |
| `s` | Cycle sort (% asc/desc, misses, filename) |
| `t` | Switch to tree view |
| `Enter` | View source coverage |
| `Esc` | Back to test tree |

### Coverage Tree

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate |
| `h` / `l` | Collapse/expand directories |
| `z` | Fold/expand all |
| `Enter` | Expand directory / drill into file source |
| `t` | Switch to table view |
| `Esc` | Back to test tree |

### Coverage Source

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll line by line |
| `Ctrl+U` / `Ctrl+D` | Half-page scroll |
| `n` | Jump to next uncovered line |
| `Esc` | Back to previous view |

### Output Panel (Tab to focus)

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll |
| `g` / `G` | Jump to top/bottom |

## How It Works

1. **Discovery** - Runs `vendor/bin/pest --list-tests` and parses the output into a tree hierarchy
2. **Execution** - Spawns Pest with `--log-junit` to get structured results, streams output in real-time
3. **Results** - Parses JUnit XML and maps results back to tree nodes using fuzzy name matching with class-based disambiguation
4. **Coverage** - Generates Clover XML via `--coverage-clover`, parses file and line-level data
5. **Watch** - Uses `notify` to watch `tests/` and `app/` directories, debounces events, and triggers targeted re-runs

Test artifacts are stored in `.pesticide/` during a session and cleaned up on exit.

## Project Structure

```
src/
├── main.rs              # Entry point, CLI args, event loop
├── app.rs               # Application state and business logic
├── watcher.rs           # File system monitoring
├── tree/
│   └── node.rs          # Test tree data structure
├── pest/
│   ├── discovery.rs     # Test discovery from Pest
│   ├── runner.rs        # Test execution and JUnit parsing
│   └── coverage.rs      # Clover XML coverage parsing
└── ui/
    ├── mod.rs           # Layout orchestration
    ├── tree.rs          # Test tree rendering
    ├── coverage_table.rs # Coverage file table
    ├── coverage_tree.rs  # Coverage directory tree
    ├── coverage_source.rs # Line-level coverage view
    ├── output.rs        # Test output panel
    └── footer.rs        # Context-sensitive keybinding help
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| [ratatui](https://ratatui.rs) | Terminal UI framework |
| [crossterm](https://docs.rs/crossterm) | Terminal control |
| [tokio](https://tokio.rs) | Async runtime |
| [notify](https://docs.rs/notify) | File system watcher |
| [roxmltree](https://docs.rs/roxmltree) | XML parsing (JUnit, Clover) |
| [clap](https://docs.rs/clap) | CLI argument parsing |
| [anyhow](https://docs.rs/anyhow) | Error handling |

## Neovim Integration

Add to `~/.config/nvim/lua/plugins/pesticide.lua` (requires [snacks.nvim](https://github.com/folke/snacks.nvim)):

```lua
local function pesticide(cmd_flags, win_opts)
  Snacks.terminal.toggle("pesticide " .. cmd_flags, {
    cwd = vim.fn.getcwd(),
    win = win_opts,
  })
end

local sidebar = { position = "right", width = 0.4, border = "left" }
local floating = { position = "float", width = 0.8, height = 0.8, border = "rounded" }

return {
  {
    "folke/snacks.nvim",
    keys = {
      { "<leader>to", function() pesticide("--watch", sidebar) end, desc = "Pesticide (sidebar)" },
      { "<leader>tr", function() pesticide("--watch --run", sidebar) end, desc = "Pesticide run all (sidebar)" },
      { "<leader>tc", function() pesticide("--watch --coverage", sidebar) end, desc = "Pesticide coverage (sidebar)" },
      { "<leader>tO", function() pesticide("", floating) end, desc = "Pesticide (floating)" },
      { "<leader>tR", function() pesticide("--run", floating) end, desc = "Pesticide run all (floating)" },
      { "<leader>tC", function() pesticide("--coverage", floating) end, desc = "Pesticide coverage (floating)" },
    },
  },
  { "folke/which-key.nvim", opts = { spec = { { "<leader>t", group = "test" } } } },
}
```

## License

MIT
