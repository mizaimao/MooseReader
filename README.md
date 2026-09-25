# MooseReader

**A blazingly fast, ultra-lightweight terminal EPUB reader written in Rust.**

MooseReader is a zero-distraction, keyboard-controlled EPUB reader designed for the terminal. Built without heavy TUI frameworks or web-rendering engines to deliver a fast reading experience with low memory profile.

![MooseReader Screenshot](assets/moosereader_sc.png)

Dedicated to Donkey.

## ✨ Features

* **Featherweight Footprint:** Very low usage of memory (only several MBs).
* **Live Layout Engine:** Dynamically adjust your reading settings on the go.
* **Multiple Themes:** Beautiful, pre-built TrueColor profiles including Dracula, Nord, Solarized, Catppuccin, Gruvbox, etc.
* **Keyboard-Native Navigation:** Keep your hands on the home row with full `h` `j` `k` `l` support.
* **Smart State Persistence:** MooseReader remembers exactly where you left off by using a percentage-based bookmarking, saved automatically as you read. Books are recognized by their content, so moving or renaming a file keeps your place.
* **Interactive Table of Contents:** A pop-up TUI pane to seamlessly navigate chapters.
* **Multi-language Interface:** Menus and messages in English, 简体中文, 繁體中文, 日本語, 한국어, Русский, Español, Français and Deutsch, following your system language unless you pick one in Settings. Chinese, Japanese and Korean books lay out by character width.
* **Customizable Footer:** Toggle chapter titles, reading progress (chapter vs. overall), percentage read, and visual progress bars `[████░░░░]`. 

## 📖 Usage
Ensure you have [Rust and Cargo](https://www.rust-lang.org/tools/install) installed on your machine.

Then clone and run!
```
git clone https://github.com/mizaimao/MooseReader.git
cd MooseReader
cargo run -- ./MyBook.epub
```

## ⌨️ Default Keybindings
|        Key        |             Action             |
|:-----------------:|:------------------------------:|
|      J / Down     |      Scroll down one line      |
|       K / Up      |       Scroll up one line       |
| L / Right / Space | Fast-forward (scroll by chunk) |
|      H / Left     |   Rewind (scroll up by chunk)  |
|        Tab        | Open / Close Table of Contents |
|         S         |       Open Settings Menu       |
|         F         |    Toggle Footer visibility    |
|       Enter       | Select Chapter / Save Settings |
|     Q / Ctrl-C    |     Save progress and Quit     |


## 🛠️ Configuration
MooseReader automatically creates its settings file at `~/.config/moosereader/config.json`. It's possible to edit it manually, or simply use the in-program Settings (hotkey: S) menu to change them on the fly. Bookmarks are saved to `~/.local/state/moosereader/bookmarks.json`, a moment after you stop scrolling and again when you quit.

Both follow the [XDG Base Directory](https://specifications.freedesktop.org/basedir-spec/latest/) convention, so `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` are respected when set. A `reader_config.json` or `bookmarks.json` left in the folder you launch from by earlier versions is picked up automatically on first run.

