# snapfire_fsr_terminal

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_terminal.svg)](https://crates.io/crates/snapfire_fsr_terminal)
[![Docs.rs](https://docs.rs/snapfire_fsr_terminal/badge.svg)](https://docs.rs/snapfire_fsr_terminal)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Draws Ink's `Box` and `Text` in a terminal, so an FSR application written with Ink runs as a terminal program. Experimental.

It lays the elements out with flexbox as Ink does and draws them as a grid of cells. Each redraw writes only the cells that changed. Each frame is sent as one synchronized update, so a terminal that supports it never shows half a frame. Wide characters such as CJK and emoji take two cells.

Most applications never depend on this crate. `fsr dev --terminal` and `fsr run` use it to run an application's root page in the terminal, with `fsr` built with its `terminal` feature. Use the crate directly to draw Ink-style layouts from Rust or to test what a terminal page draws.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_terminal = "0"
```

The crate has no features. It writes its own escape sequences and needs no terminal library.

## Quick start

```rust
use snapfire_fsr_terminal::{render, Border, Boxed, Color, Node, Text};

let node: Node = Boxed::new()
  .border(Border::Round)
  .padding_x(1)
  .child(Text::plain("Hello, ").child(Text::plain("terminal").color(Color::Ansi(2)).bold()))
  .into();
let frame = render(&node, 40);
print!("{}", frame.to_ansi());
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Build a tree of Ink elements in Rust | `Boxed`, `Text`, `Node` |
| Read the `ink-*` markup an Ink component renders in FSR | `from_markup` |
| Lay a tree out at a terminal's width | `render` |
| Print a frame once, as Ink's `renderToString` does | `Frame::to_ansi` |
| Redraw only what changed since the last frame | `Frame::diff` |
| Put a frame on screen without tearing | `Frame::update` |
| Copy to the terminal's own clipboard | `clipboard_copy`, after `DEVICE_ATTRIBUTES` and `clipboard_in_attributes` |
| Compare a frame in a test | `Frame::lines` |
| Parse a color the way Ink takes one | `Color::parse` |
