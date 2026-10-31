use snapfire_fsr_terminal::{clipboard_copy, clipboard_in_attributes, DEVICE_ATTRIBUTES};

#[test]
fn a_copy_is_osc_52_with_the_text_in_base64() {
  assert_eq!(clipboard_copy("hi"), "\x1b]52;c;aGk=\x07");
  assert_eq!(clipboard_copy("é ✓"), "\x1b]52;c;w6kg4pyT\x07");
  assert_eq!(clipboard_copy(""), "\x1b]52;c;\x07");
}

#[test]
fn the_clipboard_is_supported_when_the_device_attributes_name_52() {
  assert_eq!(DEVICE_ATTRIBUTES, "\x1b[c");
  assert_eq!(clipboard_in_attributes(b"\x1b[?62;4;22;52c"), Some(true));
  assert_eq!(clipboard_in_attributes(b"typed\x1b[?64;52;1c"), Some(true), "a reply after other input");
  assert_eq!(clipboard_in_attributes(b"\x1b[?62;22c"), Some(false));
  assert_eq!(clipboard_in_attributes(b"\x1b[?1;2c"), Some(false));
  assert_eq!(clipboard_in_attributes(b"\x1b[?62;152c"), Some(false), "a parameter is matched whole");
  assert_eq!(clipboard_in_attributes(b"\x1b[?62;52"), None, "an unfinished reply");
  assert_eq!(clipboard_in_attributes(b""), None);
}
