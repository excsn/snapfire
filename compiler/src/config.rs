use anyhow::Result;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
  pub out_dir: Option<PathBuf>,
  pub target: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
  pub compiler_options: Option<CompilerOptions>,
  pub include: Option<Vec<String>>,
  pub exclude: Option<Vec<String>>,
}

impl TsConfig {
  pub fn load(path: &Path) -> Result<Self> {
    if !path.exists() {
      // Return default if no config exists
      return Ok(TsConfig::default());
    }

    let content = fs::read_to_string(path)?;
    // We use serde_json to parse the standard JSON format
    let config: TsConfig = serde_json::from_str(&content)?;
    Ok(config)
  }
}
