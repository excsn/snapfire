use thiserror::Error;

/// A specialized `Result` type for `snapfire` operations.
pub type Result<T, E = SnapFireError> = std::result::Result<T, E>;

/// The primary error type for all `snapfire` operations.
#[derive(Debug, Error)]
pub enum SnapFireError {
  /// An error originating from the `tera` templating engine.
  #[error("Tera rendering error: {0}")]
  Tera(#[from] tera::Error),

  /// An I/O error, typically from reading template files.
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),

  /// An error that occurs when serializing a user's context.
  #[error("Context serialization error: {0}")]
  Serialization(String),

  /// An error from the file watcher, only available with the `devel` feature.
  #[cfg(feature = "devel")]
  #[error("File watcher error: {0}")]
  Watcher(#[from] notify::Error),
}