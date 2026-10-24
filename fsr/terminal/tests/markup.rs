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
fn siblings_growing_from_a_zero_basis_share_the_row_equally_as_yoga_does() {
  let row = r#"<ink-box flex-direction="row"><ink-box flex-grow="1" flex-basis="0" border-style="round"><ink-text>abcdef</ink-text></ink-box><ink-box flex-grow="1" flex-basis="0" border-style="round"><ink-text>b</ink-text></ink-box></ink-box>"#;
  let node = from_markup(row).unwrap();
  assert_eq!(render(&node, 20).lines()[0], format!("╭{0}╮╭{0}╮", "─".repeat(8)));
  let wide = from_markup(&row.replace("abcdef", "abcdefghijkl")).unwrap();
  assert_eq!(render(&wide, 20).lines()[0], format!("╭{0}╮╭{0}╮", "─".repeat(8)), "Yoga lets a flex item shrink below its content");
}

#[test]
fn a_text_that_starts_with_a_newline_keeps_its_empty_first_line() {
  let rows = r#"<ink-text><ink-text></ink-text><ink-newline></ink-newline><ink-text>a</ink-text><ink-newline></ink-newline><ink-text>b</ink-text></ink-text>"#;
  let node = from_markup(rows).unwrap();
  assert_eq!(render(&node, 10).lines(), ["", "a", "b"]);
}

#[test]
fn the_props_a_box_and_a_text_read_are_declared_with_the_values_they_take() {
  let declared = snapfire_fsr_terminal::prop_declarations();
  for line in ["export type Extent = number | `${number}%`;", "  flexBasis?: Extent;", "  paddingX?: number;", "  justifyContent?: \"flex-start\" | \"center\" | \"flex-end\" | \"space-between\" | \"space-around\" | \"space-evenly\";", "  borderColor?: string;", "  dimColor?: boolean;", "  wrap?: \"wrap\" | \"truncate\" | \"truncate-end\" | \"truncate-start\" | \"truncate-middle\";"] {
    assert!(declared.lines().any(|held| held == line), "{line} in\n{declared}");
  }
}

#[test]
fn a_box_prop_given_a_word_it_does_not_take_is_refused_naming_the_ones_it_does() {
  let refused = from_markup(r#"<ink-box flex-direction="diagonal"><ink-text>a</ink-text></ink-box>"#).unwrap_err();
  assert!(refused.contains("`flex-direction` on a Box") && refused.contains("`row`") && refused.contains("`column`") && refused.contains("`diagonal`"), "{refused}");
}

#[test]
fn every_border_style_of_ink_draws_its_cli_boxes_glyphs() {
  let drawn = |style: &str| render(&from_markup(&format!(r#"<ink-box border-style="{style}"><ink-text>x</ink-text></ink-box>"#)).unwrap(), 3).lines();
  assert_eq!(drawn("singleDouble"), ["╓─╖", "║x║", "╙─╜"]);
  assert_eq!(drawn("doubleSingle"), ["╒═╕", "│x│", "╘═╛"]);
  assert_eq!(drawn("arrow"), ["↘↓↙", "→x←", "↗↑↖"]);
}

#[test]
fn space_evenly_and_the_reverse_directions_lay_out_as_yoga_does() {
  let row = |attrs: &str, width: u16| render(&from_markup(&format!(r#"<ink-box {attrs}><ink-text>A</ink-text><ink-text>B</ink-text></ink-box>"#)).unwrap(), width).lines().iter().map(|l| l.trim_end().to_owned()).collect::<Vec<_>>();
  assert_eq!(row(r#"width="11" justify-content="space-evenly""#, 11), ["   A   B"]);
  assert_eq!(row(r#"width="4" flex-direction="row-reverse""#, 4), ["  BA"]);
  assert_eq!(row(r#"flex-direction="column-reverse""#, 4), ["B", "A"]);
}

#[test]
fn a_text_truncates_at_its_end_start_or_middle_as_cli_truncate_does() {
  let cut = |wrap: &str| render(&from_markup(&format!(r#"<ink-box width="5"><ink-text wrap="{wrap}">abcdefghij</ink-text></ink-box>"#)).unwrap(), 5).lines()[0].clone();
  assert_eq!(cut("truncate"), "abcd…");
  assert_eq!(cut("truncate-end"), "abcd…");
  assert_eq!(cut("truncate-start"), "…ghij");
  assert_eq!(cut("truncate-middle"), "ab…ij");
}
