// Derived from oxlint's loader at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use oxc_span::SourceType;

/// JavaScript or TypeScript to lint, and where it starts in the file it came from.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct JavaScriptSource<'a> {
    pub source_text: &'a str,
    pub source_type: SourceType,
    /// The byte offset of `source_text` in its file; 0 for a whole file.
    pub start: u32,
}

impl<'a> JavaScriptSource<'a> {
    pub fn new(source_text: &'a str, source_type: SourceType) -> Self {
        Self { source_text, source_type, start: 0 }
    }

    pub fn partial(source_text: &'a str, source_type: SourceType, start: u32) -> Self {
        Self { source_text, source_type, start }
    }
}
