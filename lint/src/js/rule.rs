// Derived from oxc_linter's rule.rs at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{
    AstNode,
    context::{ContextHost, LintContext},
    fixer::FixKind,
};

pub trait Rule: Sized + Default + fmt::Debug {
    /// Initialize from eslint json configuration
    fn from_configuration(_value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
        Ok(Self::default())
    }

    /// Visit each AST Node
    #[expect(unused_variables)]
    #[inline]
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {}

    /// Run only once. Useful for inspecting scopes and trivias etc.
    #[expect(unused_variables)]
    #[inline]
    fn run_once(&self, ctx: &LintContext) {}

    /// Check if a rule should be run at all.
    ///
    /// You usually do not need to implement this function. If you do, use it to
    /// enable rules on a file-by-file basis. Do not check if plugins are
    /// enabled/disabled; this is handled by the [`linter`].
    ///
    /// [`linter`]: crate::Linter
    #[expect(unused_variables)]
    #[inline]
    fn should_run(&self, ctx: &ContextHost) -> bool {
        true
    }
}

/// Pretty-print a JSON value and collapse all whitespace runs to single spaces,
/// for embedding a "received config" snippet in a rule-config deserialization error.
///
/// Kept non-generic (and out of the generic `Deserialize` impls below) so the heavy
/// `serde_json::to_string_pretty` machinery is compiled once instead of being
/// monomorphized into every rule-config `deserialize` instantiation. Returns `None`
/// when serialization fails, so callers fall back to the bare error.
fn compact_json_for_error(value: &serde_json::Value) -> Option<String> {
    serde_json::to_string_pretty(value)
        .ok()
        .map(|value_str| value_str.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Extract the first item from an ESLint-style rule configuration.
///
/// `None` represents a missing configuration (`null` or an empty array). Keeping this helper
/// non-generic avoids duplicating the array and error handling for every rule configuration type.
#[inline(never)]
fn normalize_default_rule_config(
    value: serde_json::Value,
) -> Result<Option<serde_json::Value>, serde_json::Error> {
    match value {
        serde_json::Value::Array(arr) => Ok(arr.into_iter().next()),
        serde_json::Value::Null => Ok(None),
        _ => Err(<serde_json::Error as serde::de::Error>::custom(
            "Expected array for rule configuration",
        )),
    }
}

/// Add the received configuration to a deserialization error.
#[cold]
#[inline(never)]
fn default_rule_config_error(
    error: serde_json::Error,
    value: &serde_json::Value,
) -> serde_json::Error {
    match compact_json_for_error(value) {
        Some(compact) => <serde_json::Error as serde::de::Error>::custom(format!(
            "{error}\n  received config: `{compact}`"
        )),
        None => error,
    }
}

/// A wrapper type for deserializing ESLint-style rule configurations.
///
/// ESLint configurations are typically arrays where the first element contains
/// the actual rule configuration. This type automatically extracts and deserializes
/// that first element. If the array is empty, it uses the default value.
///
/// If the configuration is invalid (e.g. does not match the expected type, has a
/// field that does not exist), it will return a deserialization error that is
/// handled by the linter and reported to the user.
///
/// # Examples
///
/// ```ignore
/// impl Rule for MyRule {
///     fn from_configuration(value: serde_json::Value) -> Result<Self, serde_json::error::Error> {
///         DefaultRuleConfig::<Self>::from_value(value).map(DefaultRuleConfig::into_inner)
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DefaultRuleConfig<T>(T);

impl<T> DefaultRuleConfig<T> {
    /// Unwraps the inner configuration value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Deserialize an ESLint-style rule configuration from a JSON value.
    pub(crate) fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let config = match normalize_default_rule_config(value)? {
            Some(value) => {
                T::deserialize(&value).map_err(|error| default_rule_config_error(error, &value))?
            }
            None => T::default(),
        };

        Ok(Self(config))
    }
}

impl<T: Default> Default for DefaultRuleConfig<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<'de, T> serde::Deserialize<'de> for DefaultRuleConfig<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(serde::de::Error::custom)
    }
}

pub trait RuleMeta {
    /// The plugin and the rule's name within it, `eslint/no-debugger`.
    const ID: &'static str;

    /// The rule's name within its plugin, `no-debugger`.
    #[cfg(test)]
    const NAME: &'static str;

    /// The plugin the rule belongs to, `eslint`.
    #[cfg(test)]
    const PLUGIN: &'static str;

    const CATEGORY: RuleCategory;

    /// Whether the plugin's recommended set turns the rule on.
    const RECOMMENDED: bool;

    /// What kind of auto-fixing can this rule do?
    const FIX: RuleFixMeta = RuleFixMeta::None;

    /// The rule's documentation, from its doc comments.
    const DOCS: &'static str;
}

/// Rule categories defined by rust-clippy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleCategory {
    /// Code that is outright wrong or useless
    Correctness,
    /// Code that is most likely wrong or useless
    Suspicious,
    /// Lints which are rather strict or have occasional false positives
    Pedantic,
    /// Code that can be written to run faster
    Perf,
    /// Code that should be written in a more idiomatic way
    Style,
    /// Lints which prevent the use of language and library features
    /// The restriction category should, emphatically, not be enabled as a whole.
    /// The contained lints may lint against perfectly reasonable code, may not have an alternative suggestion,
    /// and may contradict any other lints (including other categories).
    /// Lints should be considered on a case-by-case basis before enabling.
    Restriction,
    /// New lints that are still under development
    Nursery,
}

impl RuleCategory {

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Suspicious => "suspicious",
            Self::Pedantic => "pedantic",
            Self::Perf => "perf",
            Self::Style => "style",
            Self::Restriction => "restriction",
            Self::Nursery => "nursery",
        }
    }
}

impl TryFrom<&str> for RuleCategory {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "correctness" => Ok(Self::Correctness),
            "suspicious" => Ok(Self::Suspicious),
            "pedantic" => Ok(Self::Pedantic),
            "perf" => Ok(Self::Perf),
            "style" => Ok(Self::Style),
            "restriction" => Ok(Self::Restriction),
            "nursery" => Ok(Self::Nursery),
            _ => Err(()),
        }
    }
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let category_name = match self {
            Self::Correctness => "Correctness",
            Self::Suspicious => "Suspicious",
            Self::Pedantic => "Pedantic",
            Self::Perf => "Perf",
            Self::Style => "Style",
            Self::Restriction => "Restriction",
            Self::Nursery => "Nursery",
        };
        f.write_str(category_name)
    }
}

/// Describes the auto-fixing capabilities of a `Rule`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleFixMeta {
    /// An auto-fix is not available.
    #[default]
    None,
    /// An auto-fix could be implemented, but it has not been yet.
    FixPending,
    /// An auto-fix is available for some violations, but not all.
    Conditional(FixKind),
    /// An auto-fix is available.
    Fixable(FixKind),
}

impl RuleFixMeta {
    #[inline]
    pub fn is_none(self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    pub const fn fix_kind(self) -> FixKind {
        match self {
            Self::Conditional(kind) | Self::Fixable(kind) => {
                debug_assert!(
                    !kind.is_none(),
                    "This lint rule indicates that it provides an auto-fix but its FixKind is None. This is a bug. If this rule does not provide a fix, please use RuleFixMeta::None. Otherwise, please provide a valid FixKind"
                );
                kind
            }
            RuleFixMeta::None | RuleFixMeta::FixPending => FixKind::None,
        }
    }

    /// Does this `Rule` have some kind of auto-fix available?
    ///
    /// Also returns `true` for suggestions.
    #[inline]
    pub fn has_fix(self) -> bool {
        matches!(self, Self::Fixable(_) | Self::Conditional(_))
    }

    pub fn supports_fix(self, kind: FixKind) -> bool {
        matches!(self, Self::Fixable(fix_kind) | Self::Conditional(fix_kind) if fix_kind.can_apply(kind))
    }

    pub fn emoji(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Conditional(kind) | Self::Fixable(kind) => Some(kind.emoji()),
            Self::FixPending => Some("🚧"),
        }
    }
}

impl fmt::Display for RuleFixMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::FixPending => write!(f, "pending"),
            Self::Conditional(kind) => write!(f, "conditional_{}", kind.to_string()),
            Self::Fixable(kind) => write!(f, "fixable_{}", kind.to_string()),
        }
    }
}

impl From<RuleFixMeta> for FixKind {
    fn from(value: RuleFixMeta) -> Self {
        value.fix_kind()
    }
}

#[cfg(test)]
mod test {

    use rustc_hash::FxHashMap;

    use super::{DefaultRuleConfig, RuleCategory};

    #[test]
    fn test_deserialize_rule_category() {
        let tests = [
            ("correctness", RuleCategory::Correctness),
            ("suspicious", RuleCategory::Suspicious),
            ("restriction", RuleCategory::Restriction),
            ("perf", RuleCategory::Perf),
            ("pedantic", RuleCategory::Pedantic),
            ("style", RuleCategory::Style),
            ("nursery", RuleCategory::Nursery),
        ];

        for (input, expected) in tests {
            let de: RuleCategory = serde_json::from_str(&format!("{input:?}")).unwrap();
            // deserializes to expected value
            assert_eq!(de, expected, "{input}");
            // try_from on a str produces the same value as deserializing
            assert_eq!(de, RuleCategory::try_from(input).unwrap(), "{input}");
        }
    }

    #[test]
    fn test_deserialize_default_rule_config_single() {
        // single element present
        assert_default_rule_config("[123]", &123u32);
        assert_default_rule_config("[true]", &true);
        assert_default_rule_config("[false]", &false);

        // empty array should use defaults
        assert_default_rule_config("[]", &String::default());
    }

    #[derive(serde::Deserialize, Debug, PartialEq, Eq)]
    #[serde(default)]
    struct Obj {
        foo: String,
    }

    impl Default for Obj {
        fn default() -> Self {
            Self { foo: "defaultval".to_string() }
        }
    }

    #[test]
    fn test_deserialize_default_rule_config_object_in_array() {
        // Single-element array containing an object should parse into the object
        // configuration (fallback behavior, not the "entire-array as T" path).
        assert_default_rule_config(r#"[{ "foo": "xyz" }]"#, &Obj { foo: "xyz".to_string() });

        // Empty array -> default
        assert_default_rule_config("[]", &Obj { foo: "defaultval".to_string() });
    }

    #[derive(serde::Deserialize, Debug, PartialEq, Eq, Default)]
    #[serde(default)]
    struct ComplexConfig {
        foo: FxHashMap<String, String>,
    }

    #[test]
    fn test_deserialize_default_rule_config_with_complex_shape() {
        // A complex object shape for the rule config, like
        // `[ { "foo": { "obj": "value" } } ]`.
        assert_default_rule_config(
            r#"[ { "foo": { "obj": "value" } } ]"#,
            &ComplexConfig {
                foo: std::iter::once(("obj".to_string(), "value".to_string())).collect(),
            },
        );
    }

    #[derive(serde::Deserialize, Debug, PartialEq, Eq, Default)]
    #[serde(rename_all = "camelCase")]
    enum EnumOptions {
        #[default]
        OptionA,
        OptionB,
    }

    #[test]
    fn test_deserialize_default_rule_config_with_enum_config() {
        // A basic enum config option.
        assert_default_rule_config(r#"["optionA"]"#, &EnumOptions::OptionA);

        // Works with non-default value as well.
        assert_default_rule_config(r#"["optionB"]"#, &EnumOptions::OptionB);

        // Works with an empty array.
        assert_default_rule_config(r"[]", &EnumOptions::OptionA);
    }

    #[derive(serde::Deserialize, Debug, PartialEq, Eq)]
    #[serde(default)]
    struct ExampleObjConfig {
        baz: String,
        qux: bool,
    }

    impl Default for ExampleObjConfig {
        fn default() -> Self {
            Self { baz: "defaultbaz".to_string(), qux: false }
        }
    }

    #[test]
    fn test_deserialize_default_rule_with_object_with_multiple_fields() {
        // Test a rule config that is a simple object with multiple fields.
        assert_default_rule_config(
            r#"[{ "baz": "fooval", "qux": true }]"#,
            &ExampleObjConfig { baz: "fooval".to_string(), qux: true },
        );

        // Ensure that missing fields get their default values.
        assert_default_rule_config(
            r#"[{ "qux": true }]"#,
            &ExampleObjConfig { baz: "defaultbaz".to_string(), qux: true },
        );
    }

    // Ensure that the provided JSON deserializes into the expected value with DefaultRuleConfig.
    fn assert_default_rule_config<T>(json: &str, expected: &T)
    where
        T: serde::de::DeserializeOwned + Default + PartialEq + std::fmt::Debug,
    {
        let de: DefaultRuleConfig<T> = serde_json::from_str(json).unwrap();
        assert_eq!(de.into_inner(), *expected);
    }

}
