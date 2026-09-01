pub mod html;
pub mod json;
pub mod rows;

pub use html::{html_serialize, HtmlSession};
pub use json::{json_to_value, value_to_json, DecodeError};
pub use rows::{node_to_row_json, row_json_to_node, serialize_page};

pub const FORMAT_VERSION: u32 = 1;
