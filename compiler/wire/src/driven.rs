//! What `snapfirec --driven` and the process driving it say to each other,
//! one line at a time over the compiler's stdin and stdout, and the asset map
//! the driver hands the compiler when it owns the assets.
//!
//! The driver sends batches: one path per line, relative to the root or
//! absolute, and an empty line to end the batch; an empty batch is a full
//! rebuild. The compiler answers every batch with [`REBUILT`] or [`FAILED`].
//! When the compiler was given an asset map and a batch referenced an asset
//! the map does not name, it first writes [`REFERENCES`], one path per line
//! relative to the root and an empty line, then waits for [`MAPPED`], reads
//! the map again and compiles those sources once more before answering.
//!
//! The driver owns the framework plugins. Whenever the compiler needs one it
//! writes [`PLUGIN`] and a [`PluginRequest`] on the next line, as JSON, and
//! reads a [`PluginAnswer`] back on one line; a batch can ask any number of
//! times before it answers. One worker per extension then serves the driver's
//! own reading of a component and every compile of it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The protocol version, announced by the compiler as its first line. A
/// driver refuses another number by name rather than misreading a line.
pub const PROTOCOL: u32 = 3;

/// The compiler's first line, [`hello`] spells it.
pub const HELLO: &str = "snapfirec: driven";
pub const REBUILT: &str = "snapfirec: rebuilt";
pub const FAILED: &str = "snapfirec: failed";
/// Followed by one root-relative path per line and an empty line.
pub const REFERENCES: &str = "snapfirec: references";
/// The driver's answer once it has rewritten the map.
pub const MAPPED: &str = "mapped";
/// Followed by one line, a [`PluginRequest`] as JSON; the driver answers with one line, a [`PluginAnswer`].
pub const PLUGIN: &str = "snapfirec: plugin";

/// What the compiler asks of the plugin for an extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ask", rename_all = "lowercase")]
pub enum PluginRequest {
  /// What the plugin said of itself, which a cache key and the banner need.
  Hello { ext: String },
  /// One batch of units for the plugin to compile.
  Compile { ext: String, units: Vec<crate::Unit> },
}

/// The driver's answer to one [`PluginRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "lowercase")]
pub enum PluginAnswer {
  Hello { hello: crate::Hello },
  Compiled { outcomes: Vec<crate::Outcome> },
  /// The plugin could not start or failed to answer, with why.
  Refused { why: String },
}

/// The first line the compiler writes under `--driven`.
pub fn hello() -> String {
  format!("{HELLO} {PROTOCOL}")
}

/// The version a hello line announces, `None` for any other line.
pub fn parse_hello(line: &str) -> Option<u32> {
  line.trim_end().strip_prefix(HELLO)?.trim().parse().ok()
}

/// The map version, written into the file so a reader can refuse one it does not know.
pub const MAP_VERSION: u32 = 1;

/// Every asset the driver defines, by the path a reference names it under,
/// relative to the compiler's root with forward slashes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMap {
  pub version: u32,
  #[serde(default)]
  pub assets: BTreeMap<String, MappedAsset>,
}

impl AssetMap {
  pub fn new() -> Self {
    Self { version: MAP_VERSION, assets: BTreeMap::new() }
  }
}

/// Where one asset is served from and, for an image, the displayed size the
/// driver read from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappedAsset {
  pub url: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub width: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub height: Option<u32>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_hello_line_round_trips_and_any_other_line_is_not_one() {
    assert_eq!(parse_hello(&hello()), Some(PROTOCOL));
    assert_eq!(parse_hello("snapfirec: driven 7\n"), Some(7));
    assert_eq!(parse_hello(REBUILT), None);
    assert_eq!(parse_hello("🔥 snapfirec started"), None);
  }

  #[test]
  fn a_plugin_request_and_its_answer_each_fit_on_one_line() {
    let request = PluginRequest::Compile { ext: "vue".into(), units: vec![crate::Unit { filename: "a.vue".into(), path: "/p/a.vue".into(), source: "<template>\n<p/>\n</template>\n".into(), options: Default::default(), files: Default::default() }] };
    let line = serde_json::to_string(&request).unwrap();
    assert!(!line.contains('\n'), "{line}");
    assert!(matches!(serde_json::from_str::<PluginRequest>(&line).unwrap(), PluginRequest::Compile { ext, units } if ext == "vue" && units.len() == 1));
    let answer = PluginAnswer::Refused { why: "`snapfirec-vue` is not on PATH".into() };
    let line = serde_json::to_string(&answer).unwrap();
    assert_eq!(line, r#"{"answer":"refused","why":"`snapfirec-vue` is not on PATH"}"#);
    assert!(matches!(serde_json::from_str::<PluginAnswer>(&line).unwrap(), PluginAnswer::Refused { .. }));
  }

  #[test]
  fn a_map_serialises_without_empty_dimensions() {
    let mut map = AssetMap::new();
    map.assets.insert("fonts/a.woff2".into(), MappedAsset { url: "/s/a.1.woff2".into(), width: None, height: None });
    map.assets.insert("img/h.png".into(), MappedAsset { url: "/s/h.2.png".into(), width: Some(6), height: Some(4) });
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(json, r#"{"version":1,"assets":{"fonts/a.woff2":{"url":"/s/a.1.woff2"},"img/h.png":{"url":"/s/h.2.png","width":6,"height":4}}}"#);
    assert_eq!(serde_json::from_str::<AssetMap>(&json).unwrap(), map);
  }
}
