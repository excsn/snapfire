pub mod brace;
pub mod compare;
pub mod decode;
pub mod escape;
pub mod infer;
pub mod js;
pub mod math;
pub mod segment;
pub mod shadow;
pub mod variables;

#[cfg(test)]
mod conformance {
  use serde_json::Value;

  use super::{brace, compare, decode, escape, segment};
  use crate::compat::theme::to_key_path;

  fn string(value: &Value) -> &str {
    value.as_str().unwrap()
  }

  /// pretty-format's output for a string or a list of strings.
  fn snapshot(value: &Value) -> String {
    match value {
      Value::Array(items) => format!(
        "[\n{}]",
        items
          .iter()
          .map(|v| format!("  \"{}\",\n", string(v)))
          .collect::<String>()
      ),
      other => format!("\"{}\"", string(other)),
    }
  }

  fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
      "segment" => serde_json::json!(segment::segment(string(&args[0]), string(&args[1]).as_bytes()[0])),
      "decodeArbitraryValue" => serde_json::json!(decode::decode_arbitrary_value(string(&args[0]))),
      "expand" => serde_json::json!(brace::expand(string(&args[0])).map_err(|e| e.to_string())?),
      "escape" => serde_json::json!(escape::escape(string(&args[0]))),
      "unescape" => serde_json::json!(escape::unescape(string(&args[0]))),
      "toKeyPath" => serde_json::json!(to_key_path(string(&args[0]))),
      "compare" => serde_json::json!(compare::compare(string(&args[0]), string(&args[1])) as i8 as f64),
      "isValidStaticUtilityName" => serde_json::json!(crate::utilities::css::is_valid_static_utility_name(string(&args[0]))),
      "isValidFunctionalUtilityName" => serde_json::json!(crate::utilities::css::is_valid_functional_utility_name(string(&args[0]))),
      "compoundsForSelectors" => {
        let selectors: Vec<&str> = args[0].as_array().unwrap().iter().map(string).collect();
        serde_json::json!(f64::from(crate::variants::compounds_for_selectors(&selectors).0))
      }
      "sortCompare" => {
        let mut items: Vec<&str> = args[0].as_array().unwrap().iter().map(string).collect();
        items.sort_by(|a, z| compare::compare(a, z));
        serde_json::json!(items)
      }
      other => panic!("no helper `{other}`"),
    })
  }

  #[test]
  fn helper_fixtures() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut count = 0;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
      let path = entry.unwrap().path();
      if !path.to_string_lossy().ends_with("-units.json") {
        continue;
      }
      let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
      for case in cases {
        count += 1;
        let args = case["args"].as_array().unwrap();
        let result = call(string(&case["function"]), args);
        let actual = match (case["kind"].as_str().unwrap(), case["post"].as_str(), result) {
          ("error", _, Err(message)) => Value::String(message),
          ("error", _, Ok(value)) => Value::String(format!("no error, returned {value}")),
          (_, _, Err(message)) => Value::String(format!("error: {message}")),
          ("snapshot", _, Ok(value)) => Value::String(snapshot(&value)),
          (_, Some("sort"), Ok(Value::Array(mut items))) => {
            items.sort_by(|a, z| string(a).cmp(string(z)));
            Value::Array(items)
          }
          (_, _, Ok(value)) => value,
        };
        if actual != case["expected"] {
          failures.push(format!(
            "{} ({}): expected {}, got {actual}",
            case["test"],
            path.file_name().unwrap().to_string_lossy(),
            case["expected"]
          ));
        }
      }
    }
    assert!(count > 0);
    assert!(
      failures.is_empty(),
      "{} of {count} helper fixtures failed:\n{}",
      failures.len(),
      failures.join("\n")
    );
  }

  /// `compare.test.ts`'s "sort is stable", over every permutation from Heap's algorithm.
  #[test]
  fn sort_is_stable() {
    let mut input = vec![
      "duration-initial",
      "duration-75",
      "duration-150",
      "duration-700",
      "duration-1000",
    ];
    let expected = [
      "duration-75",
      "duration-150",
      "duration-700",
      "duration-1000",
      "duration-initial",
    ];
    let mut permutations = vec![input.clone()];
    let mut stack = vec![0; input.len()];
    let mut pos = 1;
    while pos < input.len() {
      if stack[pos] < pos {
        let k = if pos % 2 == 0 { 0 } else { stack[pos] };
        input.swap(k, pos);
        permutations.push(input.clone());
        stack[pos] += 1;
        pos = 1;
      } else {
        stack[pos] = 0;
        pos += 1;
      }
    }
    assert_eq!(permutations.len(), 120);
    for mut permutation in permutations {
      permutation.sort_by(|a, z| compare::compare(a, z));
      assert_eq!(permutation, expected);
    }
  }
}
