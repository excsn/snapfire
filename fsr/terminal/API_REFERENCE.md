# API Reference: snapfire_fsr_terminal

Ink's `Box` and `Text` laid out with flexbox and drawn to a terminal as cells.

## Contents

* [1. Elements](#1-elements)
  * [Node](#node)
  * [Boxed](#boxed)
  * [BoxStyle](#boxstyle)
  * [Extent](#extent)
  * [Direction](#direction)
  * [Justify](#justify)
  * [Align](#align)
  * [Border](#border)
  * [Text](#text)
  * [Inline](#inline)
  * [Wrap](#wrap)
* [2. Styles](#2-styles)
  * [Color](#color)
  * [Style](#style)
* [3. Drawing](#3-drawing)
  * [render](#render)
  * [render_measured](#render_measured)
  * [Metrics](#metrics)
  * [Frame](#frame)
  * [Cell](#cell)
  * [BEGIN_SYNCHRONIZED and END_SYNCHRONIZED](#begin_synchronized-and-end_synchronized)
* [4. Clipboard](#4-clipboard)
  * [clipboard_copy](#clipboard_copy)
  * [DEVICE_ATTRIBUTES](#device_attributes)
  * [clipboard_in_attributes](#clipboard_in_attributes)
* [5. Markup](#5-markup)
  * [from_markup](#from_markup)
  * [Prop](#prop)
  * [Value](#value)
  * [BOX_PROPS](#box_props)
  * [TEXT_PROPS](#text_props)
  * [prop_declarations](#prop_declarations)
* [6. Error Handling](#6-error-handling)
  * [from_markup errors](#from_markup-errors)

## 1. Elements

### Node

An element of the tree. `Debug`, `Clone`.

* `pub enum Node { Box(Boxed), Text(Text) }`
* `From<Boxed>` and `From<Text>`.

### Boxed

Ink's `Box`. `Debug`, `Clone`, `Default`.

* `pub struct Boxed { pub style: BoxStyle, pub children: Vec<Node>, pub id: Option<String> }`: `id` is the name `render_measured` reports the box under.
* `pub fn new() -> Self`
* `pub fn child(self, child: impl Into<Node>) -> Self`
* `pub fn id(self, id: impl Into<String>) -> Self`
* `pub fn column(self) -> Self`: sets `Direction::Column`.
* `pub fn gap(self, cells: u16) -> Self`
* `pub fn padding(self, cells: u16) -> Self`: all four sides.
* `pub fn padding_x(self, cells: u16) -> Self`: right and left.
* `pub fn padding_y(self, cells: u16) -> Self`: top and bottom.
* `pub fn margin(self, cells: u16) -> Self`: all four sides.
* `pub fn width(self, cells: u16) -> Self`
* `pub fn width_percent(self, percent: f32) -> Self`: `50.0` is half the parent.
* `pub fn height(self, cells: u16) -> Self`
* `pub fn height_percent(self, percent: f32) -> Self`: `50.0` is half the parent.
* `pub fn grow(self, grow: f32) -> Self`
* `pub fn justify(self, justify: Justify) -> Self`
* `pub fn align(self, align: Align) -> Self`
* `pub fn border(self, border: Border) -> Self`
* `pub fn border_color(self, color: Color) -> Self`

### BoxStyle

Ink's `Box` props, in cells. `Debug`, `Clone`, `Default`.

* `pub struct BoxStyle { pub direction: Direction, pub gap: u16, pub padding: [f32; 4], pub margin: [f32; 4], pub width: Option<Extent>, pub height: Option<Extent>, pub grow: f32, pub shrink: Option<f32>, pub basis: Option<Extent>, pub justify: Option<Justify>, pub align: Option<Align>, pub border: Option<Border>, pub border_color: Option<Color>, pub overflow_x: bool, pub overflow_y: bool, pub content_offset: (i32, i32) }`
* `padding` and `margin` are top, right, bottom and left, fractions allowed. Each box's position is rounded to whole cells.
* `shrink` is 1 when `None`. `basis` is `auto` when `None`.
* A border takes one cell on each side.
* With `overflow_x` or `overflow_y` set, what the children draw past the inner border edge on that axis is cut.
* `content_offset` draws the children that many cells left and up.

### Extent

A box's width or height. `Debug`, `Clone`, `Copy`, `PartialEq`.

* `pub enum Extent { Cells(u16), Percent(f32) }`
* `Percent(50.0)` is half the parent's size. Under a parent with no definite size, such as the root's height, the box sizes to its content.

### Direction

Ink's `flexDirection`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`.

* `pub enum Direction { Row, Column, RowReverse, ColumnReverse }`: `Row` by default.

### Justify

Ink's `justifyContent`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `pub enum Justify { Start, Center, End, SpaceBetween, SpaceAround, SpaceEvenly }`

### Align

Ink's `alignItems`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `pub enum Align { Start, Center, End, Stretch }`

### Border

Ink's `borderStyle`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `pub enum Border { Single, Double, Round, Bold, SingleDouble, DoubleSingle, Classic, Arrow }`: drawn as `┌─┐`, `╔═╗`, `╭─╮`, `┏━┓`, `╓─╖`, `╒═╕`, `+-+` and `↘↓↙`.

### Text

Ink's `Text`. `Debug`, `Clone`, `Default`.

* `pub struct Text { pub style: Style, pub wrap: Wrap, pub children: Vec<Inline> }`
* A nested `Text` adds its style to the one around it.
* An SGR sequence inside a string, `ESC [ … m`, styles what follows it and draws as nothing. A reset or a closing code returns to the `Text`'s own style. Any other escape sequence draws as nothing.
* `pub fn new() -> Self`
* `pub fn plain(text: &str) -> Self`: a `Text` holding `text`.
* `pub fn str(self, text: &str) -> Self`: appends a string.
* `pub fn child(self, text: Text) -> Self`: appends a nested `Text`.
* `pub fn color(self, color: Color) -> Self`
* `pub fn background(self, color: Color) -> Self`
* `pub fn bold(self) -> Self`
* `pub fn dim(self) -> Self`
* `pub fn italic(self) -> Self`
* `pub fn underline(self) -> Self`
* `pub fn strikethrough(self) -> Self`
* `pub fn inverse(self) -> Self`
* `pub fn truncate(self) -> Self`: sets `Wrap::Truncate`, one line ending in `…` when it does not fit.

### Inline

What a `Text` holds. `Debug`, `Clone`.

* `pub enum Inline { Str(String), Text(Text) }`: a `\n` in a string breaks the line.

### Wrap

Ink's `wrap`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`.

* `pub enum Wrap { Wrap, Truncate, TruncateStart, TruncateMiddle }`: `Wrap` by default.
* A truncated line keeps its start, its end or both halves around `…`.

## 2. Styles

### Color

A palette entry or a 24-bit color. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `pub enum Color { Ansi(u8), Rgb(u8, u8, u8) }`
* `pub fn parse(text: &str) -> Option<Self>`: `black` through `white` as `Ansi(0)` to `Ansi(7)`, their `…Bright` forms as `Ansi(8)` to `Ansi(15)`, `gray` and `grey` as `Ansi(8)`, `#rrggbb` and `rgb(r, g, b)` as `Rgb`. `text` is trimmed. `None` for anything else.

### Style

How a cell is drawn. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`.

* `pub struct Style { pub color: Option<Color>, pub background: Option<Color>, pub bold: bool, pub dim: bool, pub italic: bool, pub underline: bool, pub strikethrough: bool, pub inverse: bool }`
* `pub fn over(&self, around: &Style) -> Style`: a color this sets wins. A flag is on when either sets it.
* `pub fn is_plain(&self) -> bool`
* `pub fn sgr(&self) -> String`: `ESC[<codes>m`. The attributes come first (1 bold, 2 dim, 3 italic, 4 underline, 7 inverse, 9 strikethrough). Then the foreground (30 to 37, 90 to 97, `38;5;n`, `38;2;r;g;b`), then the background in the 40, 100 and 48 forms.

## 3. Drawing

### render

* `pub fn render(node: &Node, columns: u16) -> Frame`: lays `node` out inside a root column `columns` cells wide that stretches its children, then draws it. The frame is `columns` wide and as tall as the layout.

### render_measured

* `pub fn render_measured(node: &Node, columns: u16) -> (Frame, Vec<(String, Metrics)>)`: `render`, with the metrics of each box that has an `id`, in drawing order.

### Metrics

A box's layout as Ink's `useBoxMetrics` reports it, in cells. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`.

* `pub struct Metrics { pub width: u16, pub height: u16, pub left: i32, pub top: i32, pub client_width: u16, pub client_height: u16 }`
* `left` and `top` are from the parent's corner and leave out any content offset.
* The client size leaves out the border and keeps the padding.

### Frame

A drawn grid of cells, `width` by `height`. `Debug`, `Clone`, `PartialEq`, `Eq`.

* `pub struct Frame { pub width: usize, pub height: usize, /* private */ }`
* `pub fn new(width: usize, height: usize) -> Self`: blank cells.
* `pub fn cell(&self, x: usize, y: usize) -> Cell`: a blank cell outside the frame.
* `pub fn put(&mut self, x: usize, y: usize, ch: char, style: Style)`: a wide character takes two cells. A character that would not fit is not drawn, nor is a zero-width one.
* `pub fn lines(&self) -> Vec<String>`: each row as plain text with trailing blanks dropped.
* `pub fn to_ansi(&self) -> String`: each row with its styles, rows joined by `\n`, trailing blanks dropped, every styled run ending in `ESC[0m`.
* `pub fn diff(&self, previous: &Frame) -> String`: `ESC[<row>;<column>H` before each run of changed cells, then `ESC[<row>;1H ESC[2K` for each row of `previous` past this frame's height. Empty when the frames are equal. Positions are 1-based from the top left.
* `pub fn update(&self, previous: Option<&Frame>, cursor: Option<(u16, u16)>) -> String`: `BEGIN_SYNCHRONIZED`, then `diff(previous)`, then the cursor, then `END_SYNCHRONIZED`.
  * With `previous` `None`, `ESC[2J` and the diff against an empty frame.
  * With a `cursor` (column, row from 0), `ESC[<row>;<column>H ESC[?25h`. Without one, `ESC[?25l`.

### Cell

One cell of a frame. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Default`.

* `pub struct Cell { pub ch: char, pub style: Style, pub tail: bool }`: `tail` marks the second cell of a wide character, which its first cell draws.
* `Default` is a plain space.

### BEGIN_SYNCHRONIZED and END_SYNCHRONIZED

The start and end of a synchronized update, [mode 2026](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md).

* `pub const BEGIN_SYNCHRONIZED: &str`: `ESC[?2026h`.
* `pub const END_SYNCHRONIZED: &str`: `ESC[?2026l`.

## 4. Clipboard

### clipboard_copy

* `pub fn clipboard_copy(text: &str) -> String`: `ESC]52;c;<text as standard base64>BEL`, the [OSC 52](https://github.com/contour-terminal/vt-extensions/blob/master/clipboard-extension.md) sequence that puts `text` on the terminal's clipboard.

### DEVICE_ATTRIBUTES

* `pub const DEVICE_ATTRIBUTES: &str`: `ESC[c`, the Primary Device Attributes request.

### clipboard_in_attributes

* `pub fn clipboard_in_attributes(input: &[u8]) -> Option<bool>`: whether a `;`-separated parameter between the first `ESC[?` in `input` and the `c` after it is `52`. `None` when `input` holds no whole answer.

## 5. Markup

### from_markup

* `pub fn from_markup(markup: &str) -> Result<Node, String>`: reads `ink-box`, `ink-text`, `ink-spacer` and `ink-newline` elements, Ink's `Box`, `Text`, `Spacer` and `Newline`, with their props as attributes. Several top-level elements become one column.
* `ink-box` reads the props of `BOX_PROPS` and reads `ink-ref` as the box's `id`. `ink-text` reads the props of `TEXT_PROPS`.
* An attribute is a prop's name in kebab case or as Ink spells it. Any other attribute is ignored.
* `ink-spacer` is a box with `flex-grow` 1. `ink-newline` is a `\n`.
* `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;` and `&#x27;` are decoded. Comments are skipped.

### Prop

One prop of Ink's `Box` or `Text`.

* `pub struct Prop<S: 'static> { pub name: &'static str, pub value: Value<S> }`: `name` as Ink spells it. `value` is what the prop takes and what writing it sets on `S`.

### Value

What a prop's value is and what it sets.

* `pub enum Value<S: 'static>`, with these variants:
  * `Cells(fn(&mut S, u16))`: whole cells.
  * `Edge(fn(&mut S, f32))`: cells, fractions kept.
  * `Offset(fn(&mut S, i32))`: cells, rounded toward zero.
  * `Extent(fn(&mut S, Extent))`: cells or a percentage such as `50%`.
  * `Number(fn(&mut S, f32))`: a number.
  * `Flag(fn(&mut S, bool))`: on unless written `false`.
  * `Color(fn(&mut S, Option<Color>))`: a color `Color::parse` reads.
  * `Words(&'static [(&'static str, fn(&mut S))])`: one of the listed words.
  * `Unseen(&'static str)`: taken and drawn as nothing, declared with this TypeScript type.

### BOX_PROPS

* `pub static BOX_PROPS: &[Prop<BoxStyle>]`, holding these props:
  * `flexDirection` (`row`, `column`, `row-reverse`, `column-reverse`).
  * `gap`, `columnGap` and `rowGap`.
  * `padding`, `paddingX`, `paddingY`, `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft` and the same `margin` forms.
  * `width`, `height`, `flexGrow`, `flexShrink` and `flexBasis`.
  * `justifyContent` (`flex-start`, `center`, `flex-end`, `space-between`, `space-around`, `space-evenly`).
  * `alignItems` (`flex-start`, `center`, `flex-end`, `stretch`).
  * `borderStyle` (`single`, `double`, `round`, `bold`, `singleDouble`, `doubleSingle`, `classic`, `arrow`) and `borderColor`.
  * `overflow`, `overflowX` and `overflowY` (`visible`, `hidden`).
  * `contentOffsetX` and `contentOffsetY`.
  * `aria-label`, `aria-hidden`, `aria-role` and `aria-state`, drawn as nothing.

### TEXT_PROPS

* `pub static TEXT_PROPS: &[Prop<Text>]`, holding these props:
  * `color`, `backgroundColor`, `bold`, `dimColor`, `italic`, `underline`, `strikethrough` and `inverse`.
  * `aria-label` and `aria-hidden`, drawn as nothing.
  * `wrap` (`wrap`, `truncate`, `truncate-end`, `truncate-start`, `truncate-middle`), cut as `cli-truncate` cuts.

### prop_declarations

* `pub fn prop_declarations() -> String`: the TypeScript of `Extent`, `BoxProps` and `TextProps` from `BOX_PROPS` and `TEXT_PROPS`.
* Each prop is optional and typed by its value. A word list is a union of its words. A hyphenated name is quoted.
* `BoxProps` adds `children`, `key` and `ref`. `TextProps` adds `children` and `key`. `ReactNode` is not declared.

## 6. Error Handling

### from_markup errors

`render` and the `Frame` methods do not fail. `from_markup` returns `Err(String)` for:

* A tag that is not one of the four, such as ``"`<div>` is not an Ink element"``.
* An element inside an `ink-text` other than `ink-text` or `ink-newline`.
* An end tag that closes nothing open.
* An element never closed.
* An unquoted attribute value.
* A prop value that is not what the prop takes, such as ``"`flex-direction` on a Box is one of `row`, `column`, `row-reverse`, `column-reverse`, not `diagonal`"``.
