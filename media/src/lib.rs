//! What an image or a font file says about itself, read from its header and
//! its tables without decoding pixels. The compiler and FSR's asset pipeline
//! both read through here, so the two agree on every number.

pub mod font;
pub mod image;

pub use font::{Face, Fallback, Metrics, Style};
pub use image::Header;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("reading {0}: {1}")]
  Io(std::path::PathBuf, std::io::Error),
  #[error("{0} is not an image this build reads: {1}")]
  Image(std::path::PathBuf, String),
  #[error("{0} is not a font this build reads: {1}")]
  Font(std::path::PathBuf, String),
}
