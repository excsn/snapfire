//! `generated/assets.json`: what the build derived from the application's
//! images and fonts. The build writes it; the host reads it at boot for the
//! policy a page's `Picture` runtime needs, the font CSS, the preloads and a
//! Tera template's image lookups.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const ASSETS_FILE: &str = "generated/assets.json";

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetsManifest {
  pub version: u32,
  pub images: ImagePolicy,
  #[serde(default)]
  pub entries: Vec<ImageEntry>,
  #[serde(default)]
  pub fonts: Fonts,
}

/// The policy as the page's `Picture` runtime reads it, so the browser writes
/// the markup the server wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePolicy {
  pub widths: Vec<u32>,
  pub formats: Vec<String>,
  #[serde(default)]
  pub quality: BTreeMap<String, u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub base: Option<String>,
  #[serde(default)]
  pub sources: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageEntry {
  /// The file under the app, `src/img/hero.png`.
  pub source: String,
  /// The hashed original's URL.
  pub src: String,
  pub hash: String,
  pub width: u32,
  pub height: u32,
  /// Served as it is, with no variant.
  #[serde(default)]
  pub passthrough: bool,
  /// The widths generated, after the never-upscale rule and any override.
  #[serde(default)]
  pub widths: Vec<u32>,
  #[serde(default)]
  pub quality: BTreeMap<String, u8>,
  #[serde(default)]
  pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
  pub width: u32,
  pub format: String,
  pub url: String,
  /// Where it sits under the bundle's output directory.
  pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fonts {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub base: Option<String>,
  #[serde(default)]
  pub faces: Vec<Face>,
  /// The `@font-face` rules, the fallback faces and the `--font-*` variables.
  #[serde(default)]
  pub css: String,
  /// The face URLs to preload, in order.
  #[serde(default)]
  pub preload: Vec<String>,
  #[serde(default)]
  pub remote: Vec<Remote>,
  /// `--font-<key>` to its value.
  #[serde(default)]
  pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Face {
  pub key: String,
  pub family: String,
  pub weight: u16,
  pub style: String,
  /// The file under the app.
  pub source: String,
  pub url: String,
  /// Where the hashed copy sits under the bundle's output directory.
  pub path: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub unicode_range: Option<String>,
  #[serde(default)]
  pub preload: bool,
}

/// A provider's own stylesheet, linked rather than served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remote {
  pub key: String,
  pub family: String,
  pub href: String,
  pub preconnect: Vec<String>,
}

impl AssetsManifest {
  pub fn read(app: &Path) -> Option<AssetsManifest> {
    let text = std::fs::read_to_string(app.join(ASSETS_FILE)).ok()?;
    serde_json::from_str(&text).ok()
  }

  /// The JSON the `sf:images` meta carries, which is the policy alone.
  pub fn policy_json(&self) -> String {
    serde_json::to_string(&self.images).expect("a policy serializes")
  }

  pub fn image(&self, source: &str) -> Option<&ImageEntry> {
    self.entries.iter().find(|e| e.source == source)
  }
}
