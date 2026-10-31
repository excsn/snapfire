//! The terminal's own clipboard through OSC 52, which reaches the clipboard of the machine the user sits at, over SSH
//! too. <https://github.com/contour-terminal/vt-extensions/blob/master/clipboard-extension.md>

use base64::Engine;

/// Primary Device Attributes, `CSI c`: the terminal answers `CSI ? Ps ; … c`, and a `52` among them says it takes
/// OSC 52.
pub const DEVICE_ATTRIBUTES: &str = "\x1b[c";

/// The sequence that puts `text` on the terminal's clipboard.
pub fn clipboard_copy(text: &str) -> String {
  format!("\x1b]52;c;{}\x07", base64::engine::general_purpose::STANDARD.encode(text))
}

/// Whether the answer to [`DEVICE_ATTRIBUTES`] in `input` names `52`; `None` until `input` holds a whole answer.
pub fn clipboard_in_attributes(input: &[u8]) -> Option<bool> {
  let start = input.windows(3).position(|w| w == b"\x1b[?")?;
  let rest = &input[start + 3..];
  let end = rest.iter().position(|&b| b == b'c')?;
  Some(rest[..end].split(|&b| b == b';').any(|parameter| parameter == b"52"))
}
