use serde_json::json;
use snapfire_fsr_payload::script_json;

#[test]
fn script_json_writes_what_to_string_writes_with_every_lt_escaped() {
  let cases = [
    json!({"a": "</script><b>", "<k>": [1, 2.5, null, true, "x<y"], "nested": {"s": "\u{2028}\n\"q\"<"}}),
    json!(["<", "", "plain", {"": "<<"}]),
    json!("no lt here"),
    json!(12),
  ];
  for case in cases {
    assert_eq!(script_json(&case), case.to_string().replace('<', "\\u003c"), "{case}");
  }
}
