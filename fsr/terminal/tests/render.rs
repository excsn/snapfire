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

#[test]
fn text_keeps_its_spaces_as_ink_does() {
  let row = Text::plain("│  a   b  │");
  assert_eq!(render(&row.into(), 20).lines(), ["│  a   b  │"]);
  let indented = Boxed::new().width(6).child(Text::plain("  ab cd"));
  assert_eq!(render(&indented.into(), 20).lines()[0], "  ab");
}

#[test]
fn a_percentage_size_is_a_share_of_the_parent_and_of_an_unsized_height_nothing() {
  let node = snapfire_fsr_terminal::from_markup("<ink-box width=\"10\"><ink-box width=\"50%\"><ink-text>hello world</ink-text></ink-box><ink-text>x</ink-text></ink-box>").unwrap();
  assert_eq!(render(&node, 20).lines(), ["hellox", "world"]);
  let tall = snapfire_fsr_terminal::from_markup("<ink-box height=\"100%\"><ink-text>a</ink-text></ink-box>").unwrap();
  assert_eq!(render(&tall, 20).lines().len(), 1, "a root with no height gives a percentage nothing to share");
  assert_eq!(render(&Boxed::new().width_percent(50.0).child(Text::plain("hello world")).into(), 10).lines(), ["hello", "world"]);
}

#[test]
fn a_hidden_overflow_clips_at_the_border_and_a_content_offset_scrolls_what_it_shows() {
  let markup = "<ink-box ink-ref=\"view\" height=\"4\" overflow=\"hidden\" content-offset-y=\"1\" border-style=\"round\" flex-direction=\"column\"><ink-box ink-ref=\"content\" flex-direction=\"column\" flex-shrink=\"0\"><ink-text>a</ink-text><ink-text>b</ink-text><ink-text>c</ink-text><ink-text>d</ink-text><ink-text>e</ink-text></ink-box></ink-box>";
  let node = snapfire_fsr_terminal::from_markup(markup).unwrap();
  let (frame, boxes) = snapfire_fsr_terminal::render_measured(&node, 6);
  assert_eq!(frame.lines(), ["╭────╮", "│b   │", "│c   │", "╰────╯"]);
  let metrics = |id: &str| boxes.iter().find(|(name, _)| name == id).map(|(_, m)| *m).unwrap();
  assert_eq!(metrics("view"), snapfire_fsr_terminal::Metrics { width: 6, height: 4, left: 0, top: 0, client_width: 4, client_height: 2 });
  assert_eq!(metrics("content"), snapfire_fsr_terminal::Metrics { width: 4, height: 5, left: 1, top: 1, client_width: 4, client_height: 5 });
}
