# snapfire_fsr_terminal

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_terminal.svg)](https://crates.io/crates/snapfire_fsr_terminal)
[![Docs.rs](https://docs.rs/snapfire_fsr_terminal/badge.svg)](https://docs.rs/snapfire_fsr_terminal)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The terminal half of SnapFire FSR, experimental. It lays Ink's `Box` and `Text` out with flexbox through taffy and draws them to a terminal as cells, writing only the cells that changed on each redraw. `fsr dev --terminal` runs an Ink component with it: the lowerer turns the component's elements into `ink-*` markup, the interpreter renders and steps it and this crate reads the markup back and draws it. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_terminal = "0"
```

No features. `unicode-width` measures wide characters, which take two cells. The escape sequences are written here, with no terminal library.

## What to reach for

| Problem | Piece |
| --- | --- |
| Build a tree of Ink elements in Rust | `Boxed`, `Text`, `Node` |
| Read the markup a lowered component renders | `from_markup` |
| Lay a tree out at a terminal's width | `render` |
| Print a frame once, as Ink's `renderToString` does | `Frame::to_ansi` |
| Redraw only what changed since the last frame | `Frame::diff` |
| Compare a frame in a test | `Frame::lines` |
| Parse a color the way Ink takes one | `Color::parse` |
