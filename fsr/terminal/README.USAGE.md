# Usage Guide: snapfire_fsr_terminal

How to build Ink elements, lay them out at a terminal's width and draw them, once or as a sequence of redraws.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building Elements](#building-elements)
  * [Laying Out a Box](#laying-out-a-box)
  * [Styling Text](#styling-text)
* [Reading Lowered Markup](#reading-lowered-markup)
* [Drawing a Frame](#drawing-a-frame)
* [Redrawing What Changed](#redrawing-what-changed)
* [Error Handling](#error-handling)

## Core Concepts

* **Node**: a `Box` or a `Text`, the two elements everything else in Ink is made of.
* **Boxed**: Ink's `Box`, a flexbox container. A row unless it says otherwise.
* **Text**: Ink's `Text`, strings and nested `Text`. A nested one adds its style to the one around it.
* **Root**: the column `render` puts the tree in, as wide as the terminal, stretching its children across it the way Ink's root does.
* **Cell**: one character position. A wide character such as `日` takes two.
* **Frame**: the drawn grid of cells, as tall as what was drawn.
* **Diff**: the escape sequences that turn one frame into the next on a terminal showing the first at its top left.
* **Markup**: the `ink-box`, `ink-text`, `ink-spacer` and `ink-newline` elements a lowered component renders, with its props as kebab-case attributes.

## Quick Start

```rust
use snapfire_fsr_terminal::{render, Border, Boxed, Color, Text};

let card = Boxed::new()
  .column()
  .border(Border::Round)
  .padding_x(1)
  .child(Text::plain("Counter").bold().color(Color::parse("cyan").unwrap()))
  .child(Text::plain("Count: 3"));
let frame = render(&card.into(), 20);
assert_eq!(frame.lines()[0], "╭──────────────────╮");
print!("{}", frame.to_ansi());
```

## Building Elements

### Laying Out a Box

```rust
use snapfire_fsr_terminal::{Align, Boxed, Justify, Text};

let row = Boxed::new()
  .width(30)
  .gap(1)
  .justify(Justify::SpaceBetween)
  .align(Align::Center)
  .child(Text::plain("left"))
  .child(Boxed::new().grow(1.0))
  .child(Text::plain("right"));
```

`column`, `gap`, `padding`, `padding_x`, `padding_y`, `margin`, `width`, `height`, `grow`, `justify`, `align`, `border` and `border_color` set Ink's props of the same names. A border takes one cell on every side.

### Styling Text

```rust
use snapfire_fsr_terminal::{Color, Text};

let line = Text::plain("total ")
  .color(Color::parse("#888888").unwrap())
  .child(Text::plain("42").bold().color(Color::parse("greenBright").unwrap()));
let cut = Text::plain("a long line that will not fit").truncate();
```

Text wraps at spaces and breaks inside a word only when the word alone is wider than the line. `truncate` keeps one line and ends it in `…`.

## Reading Lowered Markup

```rust
use snapfire_fsr_terminal::{from_markup, render};

let node = from_markup(r#"<ink-box border-style="round"><ink-text bold="true">hi</ink-text></ink-box>"#)?;
let frame = render(&node, 10);
# Ok::<(), String>(())
```

Comments and `data-sf-*` markers the renderer writes are skipped. Any other tag is refused.

## Drawing a Frame

```rust
use snapfire_fsr_terminal::{render, Text};

let frame = render(&Text::plain("ok").bold().into(), 80);
assert_eq!(frame.lines(), ["ok"]);
assert_eq!(frame.to_ansi(), "\x1b[1mok\x1b[0m");
```

`lines` is plain text with trailing blanks dropped, for tests. `to_ansi` carries the styles, which is what Ink's `renderToString` returns.

## Redrawing What Changed

```rust
use snapfire_fsr_terminal::{render, Frame, Text};

let mut shown = Frame::new(0, 0);
for count in 0..3 {
  let next = render(&Text::plain(&format!("count {count}")).into(), 80);
  print!("{}", next.diff(&shown));
  shown = next;
}
```

Each changed run of cells is written after a cursor move. Rows the new frame no longer has are cleared. A diff against an empty frame writes the whole frame.

## Error Handling

`render` does not fail: a tree it cannot lay out draws an empty frame. `from_markup` answers `Err(String)` naming what it refused:

```rust
use snapfire_fsr_terminal::from_markup;

match from_markup("<div>web</div>") {
  Ok(node) => { let _ = node; }
  Err(why) => eprintln!("not Ink markup: {why}"),
}
```
