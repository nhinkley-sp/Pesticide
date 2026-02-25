# Neovim Plugin Design

## Overview

A LazyVim plugin spec (`pesticide.lua`) that integrates Pesticide into Neovim using `Snacks.terminal.toggle()`. Pesticide is a full TUI, so the plugin only manages window lifecycle and keybindings — no custom rendering.

## Approach

Uses `Snacks.terminal.toggle()` from snacks.nvim (already in the user's config). Each command launches Pesticide with different CLI flags and window configurations. Snacks handles toggle behavior, window cleanup, and terminal state.

## Keybindings

All under `<leader>t` (test) group, registered with which-key.

### Sidebar (right split, 40% width, watch mode by default)

| Key | Flags | Description |
|-----|-------|-------------|
| `<leader>to` | `--watch` | Open Pesticide sidebar |
| `<leader>tr` | `--watch --run` | Open + run all tests |
| `<leader>tc` | `--watch --coverage` | Open + run with coverage |

### Floating (centered, 80x80%, no watch)

| Key | Flags | Description |
|-----|-------|-------------|
| `<leader>tO` | (none) | Open Pesticide floating |
| `<leader>tR` | `--run` | Floating + run all tests |
| `<leader>tC` | `--coverage` | Floating + coverage |

## Window Configuration

### Sidebar
```lua
win = {
  position = "right",
  width = 0.4,
  border = "left",
}
```

### Floating
```lua
win = {
  position = "float",
  width = 0.8,
  height = 0.8,
  border = "rounded",
}
```

## Implementation

Single file: `~/.config/nvim/lua/plugins/pesticide.lua`

Returns a lazy.nvim plugin spec with:
- `keys` table defining all 6 keybindings
- Each key calls a helper function that builds the Pesticide command and calls `Snacks.terminal.toggle()`
- Working directory: `vim.fn.getcwd()`
- Which-key group: `{ t = { name = "test" } }`

## Dependencies

- snacks.nvim (already installed)
- pesticide binary in PATH (installed via `cargo install`)

## Terminal Identity

Each unique command string produces a unique terminal instance in snacks. This means:
- `<leader>to` and `<leader>tr` are separate terminals (different flags)
- Toggling `<leader>to` twice hides and re-shows the same sidebar
- Sidebar and floating use different terminal instances

## Design Decisions

1. **Sidebar = watch, floating = no watch**: Sidebar is for persistent monitoring during development. Floating is for quick one-off runs.
2. **40% width**: Consistent with Claude Code terminal split.
3. **No edgy.nvim**: Unnecessary complexity for a single sidebar tool.
4. **No custom Lua logic**: Pesticide handles all test running, tree navigation, and coverage. The plugin is purely a window wrapper.
