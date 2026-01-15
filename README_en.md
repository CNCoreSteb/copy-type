# Copy&Type

English | [简体中文](README.md)

![GitHub Release](https://img.shields.io/github/v/release/CNCoreSteb/copy-type)
![GitHub Repo stars](https://img.shields.io/github/stars/CNCoreSteb/copy-type)

A cross-platform clipboard monitor and keyboard input simulation tool, built with Rust.

## Features

- **Clipboard Monitoring**: Automatically detects and records copied text content.
- **Simulated Keyboard Input**: Simulates character-by-character keyboard input of copied content, triggered via shortcuts.
- **Format Preservation**: Fully preserves text formatting such as newlines and indentation.
- **Cross-Platform Support**: Supports Windows (primary), macOS, and Linux.
- **GUI**: Provides a graphical interface for enabling/disabling, customizing shortcuts, and previewing text to be typed.
- **Clipboard History**: Stores up to 100 items or the most recent 50MB of clipboard records.

## Use Cases

- Input copied text into fields that do not allow pasting.
- Bypass paste restrictions on certain websites.
- Scenarios requiring simulation of real keyboard input.
- Copy AI output and use it in a "human-like" way (kidding).

## Interface Features

- Enable/disable program toggle.
- Custom shortcut settings.
- Real-time display of clipboard content pending input.
- Character and line count statistics.
- Manual input trigger button.
- Clear clipboard content.
- Input speed adjustment.
- Clipboard history toggle.

## Default Shortcut

| Shortcut | Function |
|--------|------|
| `Ctrl+Shift+V` | Simulates keyboard input of clipboard content (default, customizable) |

## Installation

### Download from Releases

1. Visit the [Releases page](https://github.com/CNCoreSteb/copy-type/releases/).

2. Download the version for your OS.

3. Run it from any location you like.

### Build from Source

1. Ensure the Rust toolchain is installed:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Clone the repository and build:
   ```bash
   git clone https://github.com/CNCoreSteb/copy-type.git
   cd copy-type
   cargo build --release
   ```

3. Run the program:
   ```bash
   cargo run --release
   ```

## Usage

1. Start the program; the interface will appear.
2. Copy any text (`Ctrl+C`).
3. You will see a preview of the text to be typed in the window.
4. Place your cursor where you want the text entered.
5. Press the shortcut (default `Ctrl+Shift+V`) to trigger the simulated input.

### Modify Shortcut

1. Click the menu "Settings" -> "Shortcut Settings".
2. Or click the "Modify" button on the main interface.
3. Check the desired modifier keys (Ctrl, Shift, Alt, Win/Cmd).
4. Select the main key.
5. Click "Save".

## Configuration File

Configuration files are stored at:
- Windows: `%APPDATA%\copy-type\config.json`
- macOS: `~/Library/Application Support/copy-type/config.json`
- Linux: `~/.config/copy-type/config.json`

## Platform Dependencies

### Windows
No extra dependencies required.

### Linux
Requires X11 development libraries:
```bash
# Ubuntu/Debian
sudo apt-get install libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev

# Fedora
sudo dnf install libX11-devel libxcb-devel
```

### macOS
Requires Accessibility permissions.

## Generative AI Assistance & Vibe Coding Statement

I am a brand-new Rust beginner. This project originally started as a personal daily-use tool. Given the small scope, it inevitably relies on AI assistance during development. All code has been reviewed and tested by me to ensure it works as expected. Rust experts are welcome to submit PRs.

![](https://img.shields.io/badge/Claude%20Assisted-100%25-00a67d?logo=anthropic)
![](https://img.shields.io/badge/Gemini%20Assisted-100%25-00a67d?logo=googlegemini)

## License

GPL-3.0 License

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=CNCoreSteb/copy-type&type=date&legend=top-left)](https://www.star-history.com/#CNCoreSteb/copy-type&type=date&legend=top-left)
