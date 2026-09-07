use snapfire_fsr_macros::native;

/// What a room's transcript reads as beside the messages: figures the page
/// shows and nothing a service is needed for.
#[derive(Clone)]
pub struct Summary {
  pub words: i64,
  pub longest: String,
}

/// The application's own Rust, reached from a body as `ctx.native.digest`.
/// It holds no state and calls nothing, which is the point: a computation
/// that belongs in Rust and never wanted a service around it.
#[derive(Clone, Default)]
pub struct Digest;

#[native]
impl Digest {
  /// The word count of every message in a transcript.
  pub fn words(&self, bodies: Vec<String>) -> i64 {
    bodies.iter().map(|b| b.split_whitespace().count() as i64).sum()
  }

  /// The longest message, or an empty string when there are none.
  pub fn longest(&self, bodies: Vec<String>) -> String {
    bodies.into_iter().max_by_key(|b| b.chars().count()).unwrap_or_default()
  }
}
