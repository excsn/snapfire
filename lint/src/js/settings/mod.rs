// Ported from oxlint (crates/oxc_linter/src/config/settings/mod.rs) at oxc crates_v0.155.0, MIT License, Copyright (c) 2024-present VoidZero Inc. & Contributors.

mod jsdoc;
mod jsx_a11y;
mod react;

use serde::{Deserialize, Serialize};

use self::{jsx_a11y::JSXA11yPluginSettings, react::ReactPluginSettings};

pub use self::jsdoc::JSDocPluginSettings;

pub use self::react::ReactVersion;

/// The settings of the plugins whose rules read them, as ESLint's `settings` writes them:
/// `react` (version, link and form components), `jsx-a11y` (component mapping, polymorphic prop) and `jsdoc`
/// (tag names, the comments a rule ignores).
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct OxlintSettings {
    #[serde(skip)]
    pub json: Option<OxlintSettingsJson>,

    #[serde(default)]
    #[serde(rename = "jsx-a11y")]
    pub jsx_a11y: JSXA11yPluginSettings,

    #[serde(default)]
    pub react: ReactPluginSettings,

    #[serde(default)]
    pub jsdoc: JSDocPluginSettings,
}

#[derive(Deserialize, Default)]
struct WellKnownOxlintSettings {
    #[serde(default)]
    #[serde(rename = "jsx-a11y")]
    pub jsx_a11y: JSXA11yPluginSettings,

    #[serde(default)]
    pub react: ReactPluginSettings,

    #[serde(default)]
    pub jsdoc: JSDocPluginSettings,
}

pub type OxlintSettingsJson = serde_json::Map<String, serde_json::Value>;

impl<'de> Deserialize<'de> for OxlintSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw_value = serde_json::Value::deserialize(deserializer)?;
        let well_known_settings = WellKnownOxlintSettings::deserialize(&raw_value).map_err(serde::de::Error::custom)?;
        let arbitrary_settings = if let serde_json::Value::Object(json) = raw_value { Some(json) } else { None };
        Ok(OxlintSettings { json: arbitrary_settings, jsx_a11y: well_known_settings.jsx_a11y, react: well_known_settings.react, jsdoc: well_known_settings.jsdoc })
    }
}

#[cfg(test)]
mod test {
    use std::borrow::Cow;

    use serde::Deserialize;

    use oxc_str::CompactStr;

    use super::OxlintSettings;
    use super::react::ComponentAttrs;

    fn as_attrs<S: Into<CompactStr>, I: IntoIterator<Item = S>>(
        attrs: I,
    ) -> ComponentAttrs<'static> {
        ComponentAttrs::from(Cow::Owned(attrs.into_iter().map(Into::into).collect()))
    }

    #[test]
    fn test_parse_settings() {
        let settings = OxlintSettings::deserialize(&serde_json::json!({
            "jsx-a11y": {
                "polymorphicPropName": "role",
                "components": {
                    "Link": "Anchor",
                    "Link2": "Anchor2"
                }
            },
            "react": {
                "formComponents": [
                    "CustomForm",
                    {"name": "SimpleForm", "formAttribute": "endpoint"},
                    {"name": "Form", "formAttribute": ["registerEndpoint", "loginEndpoint"]},
                ],
                "linkComponents": [
                    "Hyperlink",
                    {"name": "MyLink", "linkAttribute": "to"},
                    {"name": "Link", "linkAttribute": ["to", "href"]},
                ]
            }
        }))
        .unwrap();

        assert_eq!(settings.jsx_a11y.polymorphic_prop_name, Some("role".into()));
        assert_eq!(settings.jsx_a11y.components.get("Link"), Some(&"Anchor".into()));
        assert_eq!(
            settings.react.get_form_component_attrs("CustomForm").unwrap(),
            as_attrs::<CompactStr, _>(vec![])
        );
        assert_eq!(
            settings.react.get_form_component_attrs("SimpleForm").unwrap(),
            as_attrs(["endpoint"])
        );
        assert_eq!(
            settings.react.get_form_component_attrs("Form").unwrap(),
            as_attrs(["registerEndpoint", "loginEndpoint"])
        );
        assert_eq!(
            settings.react.get_link_component_attrs("Link").unwrap(),
            as_attrs(["to", "href"])
        );
        assert_eq!(settings.react.get_link_component_attrs("Noop"), None);
    }

    #[test]
    fn test_parse_settings_default() {
        let settings = OxlintSettings::default();
        assert!(settings.jsx_a11y.polymorphic_prop_name.is_none());
        assert!(settings.jsx_a11y.components.is_empty());
        assert!(settings.jsx_a11y.attributes.is_empty());
    }

    #[test]
    fn test_parse_jsx_a11y_attributes() {
        let settings = OxlintSettings::deserialize(&serde_json::json!({
            "jsx-a11y": {
                "attributes": {
                    "for": ["htmlFor", "for"],
                    "class": ["className"]
                }
            }
        }))
        .unwrap();

        let for_attrs = &settings.jsx_a11y.attributes["for"];
        assert_eq!(for_attrs.len(), 2);
        assert_eq!(for_attrs[0], "htmlFor");
        assert_eq!(for_attrs[1], "for");

        let class_attrs = &settings.jsx_a11y.attributes["class"];
        assert_eq!(class_attrs.len(), 1);
        assert_eq!(class_attrs[0], "className");

        assert_eq!(settings.jsx_a11y.attributes.get("nonexistent"), None);
    }

    #[test]
    fn test_parse_jsx_a11y_attributes_empty() {
        let settings = OxlintSettings::deserialize(&serde_json::json!({
            "jsx-a11y": {
                "attributes": {}
            }
        }))
        .unwrap();

        assert!(settings.jsx_a11y.attributes.is_empty());
    }

    #[test]
    fn test_extra_fields() {
        let json_value = serde_json::json!({
            "jsx-a11y": { "polymorphicPropName": "role" },
            "unknown-plugin": { "setting": "value" },
            "globalSetting": "value"
        });

        let settings = OxlintSettings::deserialize(&json_value).unwrap();

        // Known fields are parsed correctly
        assert_eq!(settings.jsx_a11y.polymorphic_prop_name, Some("role".into()));

        // Raw JSON preserves all fields (known and unknown)
        let raw_json = settings.json.unwrap();
        assert_eq!(raw_json["jsx-a11y"]["polymorphicPropName"], "role");
        assert_eq!(raw_json["unknown-plugin"]["setting"], "value");
        assert_eq!(raw_json["globalSetting"], "value");
    }

}
