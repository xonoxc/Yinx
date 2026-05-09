# Yinx Documentation

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [TUI Navigation](#tui-navigation)
4. [Configuration](#configuration)
5. [Theme System](#theme-system)

## Installation

### Prerequisites
- Rust (latest stable)
- Cargo

### Building from Source
```bash
git clone https://github.com/username/yinx.git
cd yinx
cargo build --release
```

The binary will be available at `target/release/yinx`.

## Quick Start

### Launching Yinx
```bash
yinx
```

### Making Your First Request
1. Press `i` to enter INSERT mode
2. Type your URL in the URL bar (top)
3. Press `Tab` to move to the method selector
4. Use `Up`/`Down` arrows to select HTTP method
5. Press `Enter` to confirm method
6. Press `Ctrl+R` to execute the request

## TUI Navigation

### Pane Navigation
- `Tab` / `Shift+Tab` - Cycle panes forward/backward
- `Ctrl+1/2/3/4` - Jump to Request/Response/Workflow/Logs pane
- Click on any pane to focus it

### Layout Controls
- `F7` - Cycle layout presets (Default → Mixed → Wide → Default)
- `?` - Open/close keymap help overlay

### Pane Resizing
- `+` / `=` - Expand active pane
- `-` / `_` - Shrink active pane

Resize applies to the currently focused pane:
- **Request pane** (Wide layout): changes width
- **Response pane** (Wide layout): changes height vs logs
- **Logs pane** (Wide layout): changes height vs response

### Theme Controls
- `T` / `Shift+T` - Cycle through available themes (dark → light → ...)

### Request Pane
- `Tab` - Move between fields (Method → URL → Tabs)
- `Ctrl+F` - Open search in tab content
- `Enter` (on method) - Open method dropdown
- `Up`/`Down` - Navigate method list

## Configuration

Configuration is stored in `~/.config/yinx/config.toml` (or platform equivalent).

### Available Settings
- **theme** - Current theme ("dark", "light", or custom)
- **follow_redirects** - Automatically follow HTTP redirects (true/false)
- **timeout_seconds** - Request timeout in seconds
- **font_size** - Terminal font size
- **window_width** - Window width in characters
- **window_height** - Window height in characters

### Example Config
```toml
theme = "dark"
follow_redirects = true
timeout_seconds = 30
font_size = 14
window_width = 120
window_height = 40
```

## Theme System

Yinx supports a flexible theme system with built-in themes and custom theme support.

### Built-in Themes
- **dark** - Dark background with vibrant colors
- **light** - Light background theme
- **terminal_default** - Inherits terminal background (uses terminal's native background)

### Theme Background Behavior
When using `terminal_default` theme, Yinx inherits the terminal's background color instead of painting over it with black. This provides a native look that matches your terminal emulator.

### Creating Custom Themes

1. Create a JSON file with your theme definition:

```json
{
  "name": "my_custom",
  "background": null,
  "foreground": [240, 240, 240],
  "border": {
    "color": [100, 100, 100],
    "active_color": [97, 175, 239],
    "style": "Rounded"
  },
  "highlight": {
    "bg": [97, 175, 239],
    "fg": [0, 0, 0],
    "selected_bg": [86, 182, 194],
    "selected_fg": [0, 0, 0]
  },
  "semantic": {
    "success": [80, 200, 120],
    "error": [220, 50, 47],
    "warning": [255, 184, 108],
    "info": [97, 175, 239]
  },
  "pane": {
    "background": null,
    "title": [255, 255, 255],
    "status_bar_bg": [40, 42, 54],
    "status_bar_fg": [248, 248, 242]
  }
}
```

2. Save to `~/.config/yinx/themes/my_custom.json`
3. Settings can be opened from the command palette (future) or by editing `~/.config/yinx/config.toml` directly
4. Select "theme" and enter `my_custom`
5. Press `Enter` to apply

### Theme JSON Schema
- `background`: `null` (inherit terminal) or `[r, g, b]` array
- `foreground`: `[r, g, b]` array
- `border.style`: "Plain", "Rounded", "Double", or "Thick"
- All colors are RGB arrays with values 0-255
