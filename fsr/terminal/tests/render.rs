//! Ink's elements laid out and drawn: what each frame holds as text, as ANSI and as a redraw.

use snapfire_fsr_terminal::{render, Border, Boxed, Color, Text};

#[test]
fn a_row_lays_its_children_side_by_side_and_a_column_stacks_them() {
  let row = Boxed::new().gap(1).child(Text::plain("one")).child(Text::plain("two"));
  assert_eq!(render(&row.into(), 20).lines(), ["one two"]);
  let column = Boxed::new().column().child(Text::plain("one")).child(Text::plain("two"));
  assert_eq!(render(&column.into(), 20).lines(), ["one", "two"]);
}

#[test]
fn padding_and_a_round_border_surround_the_content() {
  let node = Boxed::new().border(Border::Round).padding_x(1).child(Text::plain("hi"));
  assert_eq!(render(&node.into(), 6).lines(), ["╭────╮", "│ hi │", "╰────╯"], "the root stretches its children across the columns, as Ink's does");
}

#[test]
fn a_growing_child_takes_the_room_its_siblings_leave() {
  let node = Boxed::new().width(10).child(Text::plain("a")).child(Boxed::new().grow(1.0).child(Text::plain("b"))).child(Text::plain("c"));
  assert_eq!(render(&node.into(), 20).lines(), ["ab       c"]);
}

#[test]
fn text_wraps_at_the_width_it_is_given_or_truncates_with_an_ellipsis() {
  let node = Boxed::new().width(5).child(Text::plain("hello world"));
  assert_eq!(render(&node.into(), 20).lines(), ["hello", "world"]);
  let cut = Boxed::new().width(5).child(Text::plain("hello world").truncate());
  assert_eq!(render(&cut.into(), 20).lines(), ["hell…"]);
}

#[test]
fn a_wide_character_takes_two_cells() {
  let node = Boxed::new().gap(1).child(Text::plain("日本")).child(Text::plain("x"));
  assert_eq!(render(&node.into(), 20).lines(), ["日本 x"]);
}

#[test]
fn colors_parse_as_ink_names_hex_and_rgb() {
  assert_eq!(Color::parse("green"), Some(Color::Ansi(2)));
  assert_eq!(Color::parse("redBright"), Some(Color::Ansi(9)));
  assert_eq!(Color::parse("gray"), Some(Color::Ansi(8)));
  assert_eq!(Color::parse("#ff8000"), Some(Color::Rgb(255, 128, 0)));
  assert_eq!(Color::parse("rgb(1, 2, 3)"), Some(Color::Rgb(1, 2, 3)));
  assert_eq!(Color::parse("chartreuse"), None);
}

#[test]
fn styled_text_writes_sgr_and_nested_text_adds_to_its_parent() {
  let node = Text::plain("ok").color(Color::Ansi(2)).bold();
  assert_eq!(render(&node.into(), 10).to_ansi(), "\x1b[1;32mok\x1b[0m");
  let nested = Text::plain("a ").child(Text::plain("b").bold()).color(Color::Rgb(1, 2, 3));
  assert_eq!(render(&nested.into(), 10).to_ansi(), "\x1b[38;2;1;2;3ma \x1b[0m\x1b[1;38;2;1;2;3mb\x1b[0m");
}

#[test]
fn a_redraw_writes_only_the_cells_that_changed_and_clears_rows_that_went_away() {
  let lines = |texts: &[&str]| {
    let mut column = Boxed::new().column();
    for text in texts {
      column = column.child(Text::plain(text));
    }
    render(&column.into(), 20)
  };
  let before = lines(&["count 1", "same", "gone"]);
  let after = lines(&["count 2", "same"]);
  assert_eq!(after.diff(&before), "\x1b[1;7H2\x1b[3;1H\x1b[2K");
  assert_eq!(after.diff(&after), "");
}
