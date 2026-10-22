//! The `ink-*` markup a lowered component renders, read back into elements and drawn.

use snapfire_fsr_terminal::{from_markup, render};

const COUNTER: &str = r#"<ink-box flex-direction="column" border-style="round" padding-x="1"><ink-text bold="true" color="green">Counter</ink-text><ink-box><ink-text>Count:</ink-text><ink-spacer></ink-spacer><ink-text color="cyan">0</ink-text></ink-box><ink-text dim-color="true">+ or up<ink-newline></ink-newline>- or down</ink-text></ink-box>"#;

#[test]
fn a_lowered_counter_reads_back_and_draws_in_its_border() {
  let node = from_markup(COUNTER).unwrap();
  let inner = |text: &str| format!("│ {text:<26} │");
  assert_eq!(
    render(&node, 30).lines(),
    [
      format!("╭{}╮", "─".repeat(28)),
      inner("Counter"),
      format!("│ Count:{}0 │", " ".repeat(19)),
      inner("+ or up"),
      inner("- or down"),
      format!("╰{}╯", "─".repeat(28)),
    ]
  );
}

#[test]
fn styles_attributes_and_entities_read_back() {
  let node = from_markup(r#"<ink-text color="cyan" bold="true" dim-color="false">a &amp; <ink-text underline="true">b</ink-text></ink-text>"#).unwrap();
  assert_eq!(render(&node, 10).to_ansi(), "\x1b[1;36ma & \x1b[0m\x1b[1;4;36mb\x1b[0m");
}

#[test]
fn comments_and_markers_the_renderer_writes_are_skipped() {
  let node = from_markup(r#"<ink-box data-sf-on="input:0"><ink-text>a<!-- -->b</ink-text></ink-box>"#).unwrap();
  assert_eq!(render(&node, 10).lines(), ["ab"]);
}

#[test]
fn markup_that_is_not_ink_is_refused() {
  assert!(from_markup("<div>x</div>").unwrap_err().contains("div"));
  assert!(from_markup("<ink-box><ink-text>x</ink-box>").is_err());
}

#[test]
fn a_fractional_margin_lays_out_and_rounds_to_cells_as_yoga_does() {
  let row = r#"<ink-box flex-direction="row"><ink-box margin-x="0.5"><ink-text>A</ink-text></ink-box><ink-box margin-x="0.5"><ink-text>B</ink-text></ink-box><ink-box margin-x="0.5"><ink-text>C</ink-text></ink-box></ink-box>"#;
  let node = from_markup(row).unwrap();
  assert_eq!(render(&node, 10).lines()[0].trim_end(), " A B C");
}

#[test]
fn a_text_that_starts_with_a_newline_keeps_its_empty_first_line() {
  let rows = r#"<ink-text><ink-text></ink-text><ink-newline></ink-newline><ink-text>a</ink-text><ink-newline></ink-newline><ink-text>b</ink-text></ink-text>"#;
  let node = from_markup(rows).unwrap();
  assert_eq!(render(&node, 10).lines(), ["", "a", "b"]);
}
