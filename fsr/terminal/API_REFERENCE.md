# API Reference: snapfire_fsr_terminal

Ink's `Box` and `Text` laid out with flexbox and drawn to a terminal as cells.

## Contents

* [1. Elements](#1-elements)
  * [Node](#node)
  * [Boxed](#boxed)
  * [BoxStyle](#boxstyle)
  * [Extent](#extent)
  * [Text](#text)
  * [Inline](#inline)
  * [Direction, Justify, Align, Border and Wrap](#direction-justify-align-border-and-wrap)
* [2. Styles](#2-styles)
  * [Color](#color)
  * [Style](#style)
* [3. Drawing](#3-drawing)
  * [render](#render)
  * [render_measured](#render_measured)
  * [Metrics](#metrics)
  * [Frame](#frame)
  * [Cell](#cell)
* [4. Markup](#4-markup)
  * [from_markup](#from_markup)
  * [Prop and Value](#prop-and-value)
* [5. Error Handling](#5-error-handling)

## 1. Elements

### Node

* `pub enum Node { Box(Boxed), Text(Text) }`, with `From<Boxed>` and `From<Text>`.

### Boxed

Ink's `Box`.

* `pub struct Boxed { pub style: BoxStyle, pub children: Vec<Node>, pub id: Option<String> }`, `Default`. `id` names the box in [render_measured](#render_measured).
* `pub fn new() -> Self`
* `pub fn child(self, child: impl Into<Node>) -> Self`
* `pub fn id(self, id: impl Into<String>) -> Self`
* `pub fn column(self) -> Self`
* `pub fn gap(self, cells: u16) -> Self`
* `pub fn padding(self, cells: u16) -> Self`, `padding_x`, `padding_y`
* `pub fn margin(self, cells: u16) -> Self`
* `pub fn width(self, cells: u16) -> Self`, `height`
* `pub fn width_percent(self, percent: f32) -> Self`, `height_percent`: `50.0` is half the parent
* `pub fn grow(self, grow: f32) -> Self`
* `pub fn justify(self, justify: Justify) -> Self`
* `pub fn align(self, align: Align) -> Self`
* `pub fn border(self, border: Border) -> Self`
* `pub fn border_color(self, color: Color) -> Self`

### BoxStyle

* `pub struct BoxStyle { pub direction: Direction, pub gap: u16, pub padding: [f32; 4], pub margin: [f32; 4], pub width: Option<Extent>, pub height: Option<Extent>, pub grow: f32, pub shrink: Option<f32>, pub justify: Option<Justify>, pub align: Option<Align>, pub border: Option<Border>, pub border_color: Option<Color>, pub overflow_x: bool, pub overflow_y: bool, pub content_offset: (i32, i32) }`, `Default`. With `overflow_x` or `overflow_y` set, what the children draw past the inner border edge on that axis is cut, as Ink's `overflow="hidden"`. `content_offset` draws the children that many cells left and up, as Ink's `contentOffsetX` and `contentOffsetY`. `padding` and `margin` are top, right, bottom and left in cells, a fraction among them; the layout rounds where each box lands to whole cells, as Yoga does for Ink. `shrink` is 1 when `None`. A border takes one cell on each side.

### Extent

* `pub enum Extent { Cells(u16), Percent(f32) }`, `Copy`, `PartialEq`: a `Box`'s width or height. `Percent(50.0)` is half the parent's size. A parent with no definite size, such as the root's height, gives a percentage nothing to share, so the box sizes to its content, as Yoga does.

### Text

Ink's `Text`. A nested `Text` adds its style to the one around it.

* `pub struct Text { pub style: Style, pub wrap: Wrap, pub children: Vec<Inline> }`, `Default`.
* `pub fn new() -> Self`, `pub fn plain(text: &str) -> Self`
* `pub fn str(self, text: &str) -> Self`, `pub fn child(self, text: Text) -> Self`
* `pub fn color(self, color: Color) -> Self`, `background`
* `pub fn bold(self) -> Self`, `dim`, `italic`, `underline`, `strikethrough`, `inverse`
* `pub fn truncate(self) -> Self`: one line ending in `…` when it does not fit.

### Inline

* `pub enum Inline { Str(String), Text(Text) }`: what a `Text` holds. A `\n` in a string breaks the line.

### Direction, Justify, Align, Border and Wrap

* `pub enum Direction { Row, Column, RowReverse, ColumnReverse }`, `Row` by default.
* `pub enum Justify { Start, Center, End, SpaceBetween, SpaceAround, SpaceEvenly }`
* `pub enum Align { Start, Center, End, Stretch }`
* `pub enum Border { Single, Double, Round, Bold, SingleDouble, DoubleSingle, Classic, Arrow }`: `┌─┐`, `╔═╗`, `╭─╮`, `┏━┓`, `╓─╖`, `╒═╕`, `+-+` and `↘↓↙`, the boxes of `cli-boxes`.
* `pub enum Wrap { Wrap, Truncate, TruncateStart, TruncateMiddle }`, `Wrap` by default. A truncated line keeps its start, its end or both halves around `…`.

## 2. Styles

### Color

* `pub enum Color { Ansi(u8), Rgb(u8, u8, u8) }`
* `pub fn parse(text: &str) -> Option<Self>`: `black` through `white` as `Ansi(0)` to `Ansi(7)`, their `…Bright` forms as `Ansi(8)` to `Ansi(15)`, `gray` and `grey` as `Ansi(8)`, `#rrggbb` and `rgb(r, g, b)` as `Rgb`. `None` for anything else.

### Style

* `pub struct Style { pub color: Option<Color>, pub background: Option<Color>, pub bold: bool, pub dim: bool, pub italic: bool, pub underline: bool, pub strikethrough: bool, pub inverse: bool }`, `Copy`, `Default`.
* `pub fn over(&self, around: &Style) -> Style`: a color this sets wins and a flag either sets is on.
* `pub fn is_plain(&self) -> bool`
* `pub fn sgr(&self) -> String`: `ESC[<codes>m` with the attributes first (1 bold, 2 dim, 3 italic, 4 underline, 7 inverse, 9 strikethrough), then the foreground (30 to 37, 90 to 97, `38;5;n`, `38;2;r;g;b`), then the background in the 40, 100 and 48 forms.

## 3. Drawing

### render

* `pub fn render(node: &Node, columns: u16) -> Frame`: lays `node` out inside a root column `columns` cells wide that stretches its children as Ink's root does, then draws it. The frame is `columns` wide and as tall as the layout.

### Frame

* `pub struct Frame { pub width: usize, pub height: usize, … }`, `Clone`, `PartialEq`.
* `pub fn new(width: usize, height: usize) -> Self`: blank cells.
* `pub fn cell(&self, x: usize, y: usize) -> Cell`: a blank cell outside the frame.
* `pub fn put(&mut self, x: usize, y: usize, ch: char, style: Style)`: a wide character takes two cells; one that would not fit is not drawn and neither is a zero-width one.
* `pub fn lines(&self) -> Vec<String>`: each row as plain text with trailing blanks dropped.
* `pub fn to_ansi(&self) -> String`: each row with its styles, rows joined by `\n`, trailing blanks dropped, every styled run ending in `ESC[0m`.
* `pub fn diff(&self, previous: &Frame) -> String`: `ESC[<row>;<column>H` before each run of changed cells, then `ESC[<row>;1H ESC[2K` for each row of `previous` past this frame's height. Empty when the frames are equal. Positions are 1-based from the top left.

### Cell

* `pub struct Cell { pub ch: char, pub style: Style, pub tail: bool }`, `Copy`. `tail` marks the second cell of a wide character, which its first cell draws.

### render_measured

* `pub fn render_measured(node: &Node, columns: u16) -> (Frame, Vec<(String, Metrics)>)`: [render](#render), with the metrics of each box that has an `id`, in drawing order.

### Metrics

* `pub struct Metrics { pub width: u16, pub height: u16, pub left: i32, pub top: i32, pub client_width: u16, pub client_height: u16 }`, `Copy`, `PartialEq`, `Default`: a box's layout as Ink's `useBoxMetrics` reports it. `left` and `top` are from the parent's corner and leave out any content offset. The client size leaves out the border and keeps the padding.

## 4. Markup

### from_markup

* `pub fn from_markup(markup: &str) -> Result<Node, String>`: reads `ink-box`, `ink-text`, `ink-spacer` and `ink-newline` with their props as kebab-case attributes, the form `snapfire_fsr_lower` lowers Ink's `Box`, `Text`, `Spacer` and `Newline` to. Several top-level elements become one column.
* `ink-box` reads the props of `BOX_PROPS` and `ink-ref` as the box's `id`; `ink-text` reads those of `TEXT_PROPS`. An attribute is a prop's name in kebab case or as Ink spells it. Any other attribute is ignored.
* `ink-spacer` is a box with `flex-grow` 1. `ink-newline` is a `\n`.
* `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;` and `&#x27;` are decoded. Comments are skipped.

### Prop and Value

* `pub struct Prop<S> { pub name: &'static str, pub value: Value<S> }`: one prop of Ink's `Box` or `Text`, named as Ink spells it, with what it takes and what writing it sets on `S`.
* `pub enum Value<S> { Cells(fn(&mut S, u16)), Edge(fn(&mut S, f32)), Offset(fn(&mut S, i32)), Extent(fn(&mut S, Extent)), Number(fn(&mut S, f32)), Flag(fn(&mut S, bool)), Color(fn(&mut S, Option<Color>)), Words(&'static [(&'static str, fn(&mut S))]), Unseen(&'static str) }`: whole cells; cells with fractions; cells rounded toward zero; cells or a percentage, `50%`; a number; a flag, on unless written `false`; a color `Color::parse` reads; one of the listed words; a prop taken and drawn as nothing, declared with the TypeScript type it holds.
* `pub static BOX_PROPS: &[Prop<BoxStyle>]`: `flexDirection` (`row`, `column`, `row-reverse`, `column-reverse`), `gap`, `columnGap`, `rowGap`, `padding`, `paddingX`, `paddingY`, `paddingTop`, `paddingRight`, `paddingBottom`, `paddingLeft`, the same `margin` forms, `width`, `height`, `flexGrow`, `flexShrink`, `flexBasis`, `justifyContent` (`flex-start`, `center`, `flex-end`, `space-between`, `space-around`, `space-evenly`), `alignItems` (`flex-start`, `center`, `flex-end`, `stretch`), `borderStyle` (the boxes of `cli-boxes`: `single`, `double`, `round`, `bold`, `singleDouble`, `doubleSingle`, `classic`, `arrow`), `borderColor`, `overflow`, `overflowX` and `overflowY` (`visible`, `hidden`), `contentOffsetX`, `contentOffsetY` and Ink's screen-reader props `aria-label`, `aria-hidden`, `aria-role` and `aria-state`, drawn as nothing.
* `pub static TEXT_PROPS: &[Prop<Text>]`: `color`, `backgroundColor`, `bold`, `dimColor`, `italic`, `underline`, `strikethrough`, `inverse`, `aria-label` and `aria-hidden` (drawn as nothing) and `wrap` (`wrap`, `truncate`, `truncate-end`, `truncate-start`, `truncate-middle`, cut as `cli-truncate` cuts).
* `pub fn prop_declarations() -> String`: the TypeScript of `Extent`, `BoxProps` and `TextProps` from the two tables, each prop optional and typed by its value, a word list as a union of its words, a hyphenated name quoted. `BoxProps` adds `children`, `key` and `ref`, `TextProps` `children` and `key`; `ReactNode` is the importing file's.

## 5. Error Handling

`render` and the `Frame` methods do not fail. `from_markup` answers `Err(String)` for a tag that is not one of the four (``"`<div>` is not an Ink element"``), an element inside an `ink-text` other than `ink-text` or `ink-newline`, an end tag that closes nothing open, an element never closed, an unquoted attribute value and a prop's value that is not what the prop takes, ``"`flex-direction` on a Box is one of `row`, `column`, `row-reverse`, `column-reverse`, not `diagonal`"``.
