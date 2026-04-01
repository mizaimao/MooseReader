# MooseReader

**A blazingly fast, ultra-lightweight terminal EPUB reader written in Rust.**

MooseReader is a zero-distraction, keyboard-driven EPUB reader designed for the terminal. Built entirely from scratch on top of `crossterm`, it bypasses heavy TUI frameworks and web-rendering engines to deliver a fast reading experience with low memory profile.

![MooseReader Screenshot](link-to-your-screenshot.png)

Dedicated to Donkey.

## ✨ Features

* **Featherweight Footprint:** Idles at very low usage of memory (MBs). No headless browsers, no bloated DOM trees. 
* **Live Layout Engine:** Dynamically adjust your reading width, left/right margins, and scroll speeds via an interactive TUI overlay. The text re-wraps instantly in the background.
* **Multiple Curated Color Themes:** Beautiful, pre-built TrueColor profiles including Dracula, Nord, Solarized, Catppuccin, Gruvbox, and pure Terminal native.
* **Vim-Native Navigation:** Keep your hands on the home row with full `h` `j` `k` `l` support.
* **Smart State Persistence:** MooseReader remembers exactly where you left off. Using percentage-based bookmarking.
* **Interactive Table of Contents:** A pop-up TUI pane to seamlessly navigate chapters.
* **Customizable Footer:** Toggle chapter titles, reading progress (chapter vs. overall), percentage read, and visual progress bars `[████░░░░]`. 

## 🚀 Installation

Ensure you have [Rust and Cargo](https://www.rust-lang.org/tools/install) installed on your machine.

```bash
git clone [https://github.com/YOUR_USERNAME/MooseReader.git](https://github.com/YOUR_USERNAME/MooseReader.git)
cd MooseReader
cargo build --release
```

## 📖 Usage
Simply pass the path of your EPUB file to the application:
```
cargo run -- MyBook.epub
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
|         Q         |     Save progress and Quit     |


## 🛠️ Configuration
MooseReader automatically creates a reader_config.json file in its cloned directory. It's possible to edit it manually, or simply use the in-program Settings (hotkey: S) menu to change them on the fly. Bookmarks are saved to a local bookmarks.json file.

