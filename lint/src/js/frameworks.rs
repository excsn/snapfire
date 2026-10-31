// Ported from oxlint (crates/oxc_linter/src/frameworks.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

/// The framework context a script block is linted in.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum FrameworkOptions {
    #[default]
    Default,
    /// Inside a Vue component's `<script setup>`.
    VueSetup,
}
