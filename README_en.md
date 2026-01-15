# Copy&Type

English | [简体中文](README.md)

A cross-platform clipboard monitor and keyboard input simulation tool, built with Rust.

## Features

* **Clipboard Monitoring**: Automatically detects and records copied text content.
* **Simulated Keyboard Input**: Simulates character-by-character keyboard input of copied content, triggered via shortcuts.
* **Format Preservation**: Fully preserves text formatting such as newlines and indentation.
* **Cross-Platform Support**: Supports Windows (Primary), macOS, and Linux.
* **GUI**: Provides a graphical interface for enabling/disabling, customizing shortcuts, and previewing text to be typed.
* **Clipboard History**: Stores up to 100 recent clipboard entries (Usage currently not recommended).

## Use Cases

* Inputting copied text into fields that do not allow pasting.
* Bypassing paste restrictions on certain websites.
* Scenarios requiring the simulation of real keyboard input.
* Copying AI output and using it in a "human-like" manner (kidding).

## Interface Features

* Enable/Disable program toggle.
* Custom shortcut settings.
* Real-time display of clipboard content pending input.
* Character and line count statistics.
* Manual input trigger button.
* Clear clipboard content.
* Input speed adjustment.
* Clipboard history toggle.

## Default Shortcuts

| Shortcut | Function |
| --- | --- |
| `Ctrl+Shift+V` | Simulates keyboard input of clipboard content (Default, customizable) |

## Installation

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
3. You will see a preview of the text to be input in the window.
4. Place your cursor in the location where you want the text entered.
5. Press the shortcut (Default `Ctrl+Shift+V`) to trigger the simulated input.

### Modifying Shortcuts

1. Click the menu "Settings" -> "Shortcut Settings".
2. Or click the "Modify" button on the main interface.
3. Check the desired modifier keys (Ctrl, Shift, Alt, Win/Cmd).
4. Select the main key.
5. Click "Save".

## Configuration File

Configuration files are stored at:

* **Windows**: `%APPDATA%\copy-type\config.json`
* **macOS**: `~/Library/Application Support/copy-type/config.json`
* **Linux**: `~/.config/copy-type/config.json`

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

## License

GPL-3.0 License