//! What the `/auth/` routes and the action routes share: redirects, same-origin paths and the
//! parameters a callback carries.

use super::*;

pub(crate) fn see_other(location: &str) -> Response<Body> {
  match Response::builder()
    .status(StatusCode::SEE_OTHER)
    .header(header::LOCATION, location)
    .body(Body::default())
  {
    Ok(response) => response,
    Err(_) => text_response(StatusCode::BAD_REQUEST, format!("`{location}` is not a location")),
  }
}

/// A path on this origin: `/` first and not a scheme-relative `//host`.
pub(crate) fn same_origin_path(candidate: &str) -> Option<String> {
  let own = candidate.starts_with('/') && !candidate.starts_with("//") && !candidate.starts_with("/\\");
  own.then(|| candidate.to_owned())
}

/// The path and query of a `Referer`, whether it came absolute or bare.
/// The path of a `Referer` on this origin, which is where a form post lands
/// again. `None` for another origin.
pub fn referer_path(referer: &str) -> Option<String> {
  if referer.starts_with('/') {
    return same_origin_path(referer);
  }
  let rest = referer.split_once("://")?.1;
  same_origin_path(rest.find('/').map(|i| &rest[i..]).unwrap_or("/"))
}
