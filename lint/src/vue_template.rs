//! eslint-plugin-vue's `vue3-essential` template rules over the tree `snapfirec-vue` describes: one object per
//! node, an element's `kind` 0 for an element, 1 a component, 2 a slot and 3 a `<template>`, each prop an
//! `attribute` or a `directive` with its `arg`, `exp` and `modifiers`.

use serde_json::Value;

use crate::{Level, RuleInfo};
use crate::templates::Found;

macro_rules! rule {
  ($id:literal, $what:literal, $why:literal) => {
    RuleInfo { id: $id, default: Level::Error, recommended: true, category: "correctness", fixable: false, docs: concat!("### What it does\n\n", $what, "\n\n### Why is this bad?\n\n", $why, "\n") }
  };
}

pub(crate) const RULES: [RuleInfo; 38] = [
  rule!("vue/no-child-content", "Disallows children on an element whose `v-html` or `v-text` replaces them.", "The directive overwrites the children, so they never show."),
  rule!("vue/no-deprecated-dollar-listeners-api", "Disallows `$listeners` in a template.", "Vue 3 removed it; listeners are part of `$attrs`."),
  rule!("vue/no-deprecated-dollar-scopedslots-api", "Disallows `$scopedSlots` in a template.", "Vue 3 removed it; every slot is in `$slots`."),
  rule!("vue/no-deprecated-html-element-is", "Disallows `is` on an HTML element unless its value starts with `vue:`.", "Vue 3 reads `is` on a native element as the custom element attribute."),
  rule!("vue/no-deprecated-inline-template", "Disallows the `inline-template` attribute.", "Vue 3 removed it."),
  rule!("vue/no-deprecated-router-link-tag-prop", "Disallows `tag` on `<router-link>`.", "Vue Router 4 removed it; use the `v-slot` API."),
  rule!("vue/no-deprecated-scope-attribute", "Disallows `scope` on a `<template>`.", "Vue 3 removed it; use `v-slot`."),
  rule!("vue/no-deprecated-slot-attribute", "Disallows the `slot` attribute.", "Vue 3 removed it; use `v-slot`."),
  rule!("vue/no-deprecated-slot-scope-attribute", "Disallows the `slot-scope` attribute.", "Vue 3 removed it; use `v-slot`."),
  rule!("vue/no-deprecated-v-bind-sync", "Disallows the `.sync` modifier on `v-bind`.", "Vue 3 removed it; use `v-model:prop`."),
  rule!("vue/no-deprecated-v-is", "Disallows `v-is`.", "Vue 3.1 deprecated it for `is=\"vue:...\"`."),
  rule!("vue/no-deprecated-v-on-native-modifier", "Disallows the `.native` modifier on `v-on`.", "Vue 3 removed it; a listener not declared in `emits` reaches the root element."),
  rule!("vue/no-deprecated-v-on-number-modifiers", "Disallows a key code as a `v-on` modifier, `@keyup.13`.", "Vue 3 removed key codes; use the key's name, `@keyup.enter`."),
  rule!("vue/no-dupe-v-else-if", "Disallows a `v-else-if` testing what an earlier branch of its chain tested.", "The branch can never run."),
  rule!("vue/no-duplicate-attributes", "Disallows the same attribute twice on an element, written or bound. `class` and `style` may be both.", "Only one value is used; the other is lost."),
  rule!("vue/no-textarea-mustache", "Disallows `{{ }}` inside `<textarea>`.", "It is not interpolated there; bind the text with `v-model`."),
  rule!("vue/no-useless-template-attributes", "Disallows attributes on a `<template>` other than its directives.", "A `<template>` renders nothing of its own, so the attribute goes nowhere."),
  rule!("vue/no-v-for-template-key-on-child", "Disallows `:key` on a child of `<template v-for>`.", "Vue 3 wants the key on the `<template>` itself."),
  rule!("vue/no-v-text-v-html-on-component", "Disallows `v-text` and `v-html` on a component.", "They overwrite the component's own rendering."),
  rule!("vue/require-toggle-inside-transition", "Requires the element inside `<transition>` to have `v-if` or `v-show`, unless the transition has `appear`.", "Nothing toggles, so the transition never runs."),
  rule!("vue/use-v-on-exact", "Requires `.exact` on a listener when another on the same event takes system modifiers.", "Without it the plain listener also runs with the modifiers held."),
  rule!("vue/valid-attribute-name", "Requires an attribute name to be a valid name.", "The browser drops it or the compiler refuses it."),
  rule!("vue/valid-template-root", "Requires a component's `<template>` to have content.", "An empty template renders nothing."),
  rule!("vue/valid-v-bind", "Requires `v-bind` to have an argument or a value and only `.prop`, `.camel` and `.attr` as modifiers.", "Anything else does nothing or fails to compile."),
  rule!("vue/valid-v-cloak", "Requires `v-cloak` to have no argument, modifier or value.", "It takes none."),
  rule!("vue/valid-v-else-if", "Requires `v-else-if` to have a value and no argument or modifier, and to follow a `v-if` or `v-else-if`, without `v-if` or `v-else` beside it.", "Otherwise the chain is broken and the compiler refuses it."),
  rule!("vue/valid-v-else", "Requires `v-else` to have no argument, modifier or value, and to follow a `v-if` or `v-else-if`, without `v-if` or `v-else-if` beside it.", "Otherwise the chain is broken and the compiler refuses it."),
  rule!("vue/valid-v-for", "Requires `v-for` to read `item in items`, `(item, index) in items` or the same with `of`.", "Anything else fails to compile."),
  rule!("vue/valid-v-html", "Requires `v-html` to have a value and no argument or modifier.", "It renders nothing without one."),
  rule!("vue/valid-v-if", "Requires `v-if` to have a value and no argument or modifier, without `v-else` or `v-else-if` beside it.", "Otherwise the compiler refuses it."),
  rule!("vue/valid-v-memo", "Requires `v-memo` to have an array value and no argument or modifier.", "It compares the array's items; anything else memoizes nothing."),
  rule!("vue/valid-v-model", "Requires `v-model` on an input, a select, a textarea or a component, with a value that can be assigned and no argument on a native element.", "Otherwise there is nothing to write the value back to."),
  rule!("vue/valid-v-on", "Requires `v-on` to have a value or a `.prevent` or `.stop` modifier, and an argument or an object value.", "A listener with neither does nothing."),
  rule!("vue/valid-v-once", "Requires `v-once` to have no argument, modifier or value.", "It takes none."),
  rule!("vue/valid-v-show", "Requires `v-show` to have a value and no argument or modifier, and not to be on a `<template>`.", "A `<template>` has no element to hide."),
  rule!("vue/valid-v-slot", "Requires `v-slot` on a component or on a `<template>` directly inside one, with no modifiers.", "Anywhere else there is no slot to fill."),
  rule!("vue/valid-v-text", "Requires `v-text` to have a value and no argument or modifier.", "It renders nothing without one."),
  rule!("vue/valid-v-is", "Requires `v-is` to have a value and no argument or modifier, and not to be on a component.", "Otherwise there is nothing to resolve."),
];

/// The findings of these rules and the four in [`crate::templates`] over a described template; `source` places a
/// finding about the whole template.
pub(crate) fn lint(template: &Value, source: &str, out: &mut Vec<Found>) {
  let children = template["children"].as_array().map(Vec::as_slice).unwrap_or_default();
  if !children.iter().any(|child| child["node"] != "text" || child["content"].as_str().is_some_and(|t| !t.trim().is_empty())) {
    let (line, column) = place(source, source.find("<template").unwrap_or(0));
    out.push(Found { rule: "vue/valid-template-root".to_owned(), line, column, message: "the template is empty".to_owned(), help: Some("give the component something to render".to_owned()) });
  }
  siblings(children, None, out);
}

fn place(source: &str, offset: usize) -> (usize, usize) {
  let before = &source[..offset.min(source.len())];
  (before.matches('\n').count() + 1, before.rsplit('\n').next().map_or(0, str::len) + 1)
}

fn props(node: &Value) -> &[Value] {
  node["props"].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn directive<'v>(node: &'v Value, name: &str) -> Option<&'v Value> {
  props(node).iter().find(|p| p["prop"] == "directive" && p["name"] == name)
}

fn attribute<'v>(node: &'v Value, name: &str) -> Option<&'v Value> {
  props(node).iter().find(|p| p["prop"] == "attribute" && p["name"] == name)
}

fn bound<'v>(node: &'v Value, name: &str) -> Option<&'v Value> {
  props(node).iter().find(|p| p["prop"] == "directive" && p["name"] == "bind" && p["arg"] == name)
}

fn modifiers(prop: &Value) -> Vec<&str> {
  prop["modifiers"].as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

fn exp(prop: &Value) -> Option<&str> {
  prop["exp"].as_str().filter(|e| !e.trim().is_empty())
}

fn find(out: &mut Vec<Found>, at: &Value, rule: &str, message: String) {
  out.push(Found { rule: rule.to_owned(), line: at["line"].as_u64().unwrap_or(1) as usize, column: at["column"].as_u64().unwrap_or(1) as usize, message, help: None });
}

/// Walks `children` in order, each element judged with the element before it, skipping whitespace text.
fn siblings(children: &[Value], parent: Option<&Value>, out: &mut Vec<Found>) {
  let mut previous: Option<&Value> = None;
  let mut chain: Vec<String> = Vec::new();
  for child in children {
    match child["node"].as_str() {
      Some("element") => {
        element(child, parent, previous, &mut chain, out);
        siblings(child["children"].as_array().map(Vec::as_slice).unwrap_or_default(), Some(child), out);
        previous = Some(child);
      }
      Some("text") if child["content"].as_str().is_some_and(|t| t.trim().is_empty()) => {}
      Some("interpolation") => {
        if parent.is_some_and(|p| p["tag"] == "textarea") {
          find(out, child, "vue/no-textarea-mustache", "`{{ }}` inside `<textarea>` is not interpolated".to_owned());
        }
        deprecated_apis(child["content"].as_str(), child, out);
        previous = None;
      }
      _ => previous = None,
    }
  }
}

const SYSTEM_MODIFIERS: [&str; 4] = ["ctrl", "shift", "alt", "meta"];

fn element(node: &Value, parent: Option<&Value>, previous: Option<&Value>, chain: &mut Vec<String>, out: &mut Vec<Found>) {
  let tag = node["tag"].as_str().unwrap_or_default();
  let kind = node["kind"].as_u64().unwrap_or(0);
  let component = kind == 1;
  let follows_if = previous.is_some_and(|p| directive(p, "if").is_some() || directive(p, "else-if").is_some());
  let (v_if, v_else_if, v_else) = (directive(node, "if"), directive(node, "else-if"), directive(node, "else"));
  let bare = |p: &Value| p["arg"].is_null() && modifiers(p).is_empty();
  if let Some(p) = v_if {
    if !bare(p) || exp(p).is_none() || v_else.is_some() || v_else_if.is_some() {
      find(out, p, "vue/valid-v-if", "`v-if` needs a value, no argument or modifier and no `v-else` beside it".to_owned());
    }
    *chain = exp(p).map(|e| vec![normal(e)]).unwrap_or_default();
  }
  if let Some(p) = v_else_if {
    if !bare(p) || exp(p).is_none() || !follows_if || v_if.is_some() || v_else.is_some() {
      find(out, p, "vue/valid-v-else-if", "`v-else-if` needs a value and to follow a `v-if` or `v-else-if`".to_owned());
    }
    if let Some(e) = exp(p).map(normal) {
      if follows_if && chain.contains(&e) {
        find(out, p, "vue/no-dupe-v-else-if", format!("`{e}` is tested earlier in this chain"));
      }
      chain.push(e);
    }
  }
  if let Some(p) = v_else {
    if !bare(p) || exp(p).is_some() || !follows_if || v_if.is_some() || v_else_if.is_some() {
      find(out, p, "vue/valid-v-else", "`v-else` takes no value and follows a `v-if` or `v-else-if`".to_owned());
    }
  }
  if v_if.is_none() && v_else_if.is_none() {
    chain.clear();
  }
  for p in props(node) {
    let name = p["name"].as_str().unwrap_or_default();
    if p["prop"] == "attribute" {
      attribute_rules(p, name, tag, kind, out);
      continue;
    }
    deprecated_apis(exp(p), p, out);
    let mods = modifiers(p);
    match name {
      "for" => {
        let valid = exp(p).is_some_and(|e| lazy_regex::regex_is_match!(r"^\s*(?:\([^)]*\)|[A-Za-z_$][\w$]*|\{[^}]*\}|\[[^\]]*\])\s+(?:in|of)\s+\S", e));
        if !valid || !bare(p) {
          find(out, p, "vue/valid-v-for", "`v-for` reads `item in items` or `(item, index) in items`".to_owned());
        }
      }
      "show" | "html" | "text" => {
        if !bare(p) || exp(p).is_none() || (name == "show" && kind == 3) {
          find(out, p, &format!("vue/valid-v-{name}"), format!("`v-{name}` needs a value and no argument or modifier"));
        }
        if name != "show" && component {
          find(out, p, "vue/no-v-text-v-html-on-component", format!("`v-{name}` on the component `<{tag}>` overwrites what it renders"));
        }
        if name != "show" && node["children"].as_array().is_some_and(|c| c.iter().any(|c| c["node"] != "text" || c["content"].as_str().is_some_and(|t| !t.trim().is_empty()))) {
          find(out, p, "vue/no-child-content", format!("`v-{name}` replaces the children of `<{tag}>`"));
        }
      }
      "once" | "cloak" => {
        if !bare(p) || exp(p).is_some() {
          find(out, p, &format!("vue/valid-v-{name}"), format!("`v-{name}` takes no argument, modifier or value"));
        }
      }
      "bind" => {
        if mods.contains(&"sync") {
          find(out, p, "vue/no-deprecated-v-bind-sync", "`.sync` is gone in Vue 3; use `v-model:prop`".to_owned());
        }
        let known = mods.iter().all(|m| matches!(*m, "prop" | "camel" | "attr" | "sync"));
        if !known || (p["arg"].is_null() && exp(p).is_none()) {
          find(out, p, "vue/valid-v-bind", "`v-bind` needs an argument or a value and takes only `.prop`, `.camel` and `.attr`".to_owned());
        }
        if let Some(arg) = p["arg"].as_str().filter(|_| p["argStatic"] != false) {
          if !valid_name(arg) {
            find(out, p, "vue/valid-attribute-name", format!("`{arg}` is not a valid attribute name"));
          }
        }
      }
      "on" => {
        if mods.contains(&"native") {
          find(out, p, "vue/no-deprecated-v-on-native-modifier", "`.native` is gone in Vue 3".to_owned());
        }
        if mods.iter().any(|m| m.chars().all(|c| c.is_ascii_digit())) {
          find(out, p, "vue/no-deprecated-v-on-number-modifiers", "a key code modifier is gone in Vue 3; use the key's name".to_owned());
        }
        if (exp(p).is_none() && !mods.iter().any(|m| matches!(*m, "prevent" | "stop"))) || (p["arg"].is_null() && exp(p).is_none()) {
          find(out, p, "vue/valid-v-on", "`v-on` needs a value or a `.prevent` or `.stop` modifier".to_owned());
        }
        if !mods.contains(&"exact") && !mods.iter().any(|m| SYSTEM_MODIFIERS.contains(m)) {
          let others = props(node).iter().filter(|o| o["prop"] == "directive" && o["name"] == "on" && o["arg"] == p["arg"]);
          if others.into_iter().any(|o| modifiers(o).iter().any(|m| SYSTEM_MODIFIERS.contains(m))) {
            find(out, p, "vue/use-v-on-exact", format!("add `.exact`: another `{}` listener takes system modifiers", p["arg"].as_str().unwrap_or("v-on")));
          }
        }
      }
      "model" => {
        let native = matches!(tag, "input" | "select" | "textarea");
        let assignable = exp(p).is_some_and(|e| lazy_regex::regex_is_match!(r"^\s*[A-Za-z_$][\w$]*(?:\s*(?:\??\.\s*[A-Za-z_$][\w$]*|\[[^\]]+\]))*\s*$", e));
        let file = tag == "input" && attribute(node, "type").is_some_and(|t| t["value"] == "file");
        if !(native || component) || !assignable || (native && !p["arg"].is_null()) || file {
          find(out, p, "vue/valid-v-model", "`v-model` needs an input, a select, a textarea or a component and a value it can assign".to_owned());
        }
      }
      "slot" => {
        let on_template_in_component = kind == 3 && parent.is_some_and(|p| p["kind"] == 1);
        if !(component || on_template_in_component) || !mods.is_empty() {
          find(out, p, "vue/valid-v-slot", "`v-slot` goes on a component or a `<template>` directly inside one".to_owned());
        }
      }
      "memo" => {
        if !bare(p) || !exp(p).is_some_and(|e| !lazy_regex::regex_is_match!(r#"^\s*(?:\d|['"`]|true\b|false\b|null\b|\{)"#, e)) {
          find(out, p, "vue/valid-v-memo", "`v-memo` needs an array of values".to_owned());
        }
      }
      "is" => {
        find(out, p, "vue/no-deprecated-v-is", "use `is=\"vue:...\"` in place of `v-is`".to_owned());
        if !bare(p) || exp(p).is_none() || component {
          find(out, p, "vue/valid-v-is", "`v-is` needs a value and no argument or modifier".to_owned());
        }
      }
      _ => {}
    }
  }
  duplicates(node, out);
  if kind == 3 {
    for p in props(node).iter().filter(|p| p["prop"] == "attribute" || (p["name"] == "bind" && p["arg"] != "key")) {
      if matches!(p["name"].as_str(), Some("key" | "slot-scope" | "scope" | "slot")) {
        continue;
      }
      find(out, p, "vue/no-useless-template-attributes", format!("`{}` on a `<template>` goes nowhere", p["arg"].as_str().or(p["name"].as_str()).unwrap_or_default()));
    }
    if directive(node, "for").is_some() {
      for child in node["children"].as_array().into_iter().flatten().filter(|c| c["node"] == "element") {
        if let Some(key) = bound(child, "key").or_else(|| attribute(child, "key")) {
          find(out, key, "vue/no-v-for-template-key-on-child", "put the key on the `<template v-for>` itself".to_owned());
        }
      }
    }
  }
  if matches!(tag, "transition" | "Transition") && attribute(node, "appear").is_none() && bound(node, "appear").is_none() {
    for child in node["children"].as_array().into_iter().flatten().filter(|c| c["node"] == "element" && c["kind"] != 1) {
      if directive(child, "if").is_none() && directive(child, "show").is_none() {
        find(out, child, "vue/require-toggle-inside-transition", "the element inside `<transition>` needs `v-if` or `v-show`".to_owned());
      }
    }
  }
}

fn attribute_rules(p: &Value, name: &str, tag: &str, kind: u64, out: &mut Vec<Found>) {
  if !valid_name(name) {
    find(out, p, "vue/valid-attribute-name", format!("`{name}` is not a valid attribute name"));
  }
  match name {
    "slot" => find(out, p, "vue/no-deprecated-slot-attribute", "`slot` is gone in Vue 3; use `v-slot`".to_owned()),
    "slot-scope" => find(out, p, "vue/no-deprecated-slot-scope-attribute", "`slot-scope` is gone in Vue 3; use `v-slot`".to_owned()),
    "scope" if tag == "template" => find(out, p, "vue/no-deprecated-scope-attribute", "`scope` is gone in Vue 3; use `v-slot`".to_owned()),
    "inline-template" => find(out, p, "vue/no-deprecated-inline-template", "`inline-template` is gone in Vue 3".to_owned()),
    "is" if kind == 0 && !p["value"].as_str().is_some_and(|v| v.starts_with("vue:")) && tag != "component" => {
      find(out, p, "vue/no-deprecated-html-element-is", format!("`is` on `<{tag}>` needs a `vue:` prefix to name a component"));
    }
    "tag" if matches!(tag, "router-link" | "RouterLink") => find(out, p, "vue/no-deprecated-router-link-tag-prop", "`tag` is gone from `<router-link>`; use its `v-slot`".to_owned()),
    _ => {}
  }
}

fn deprecated_apis(text: Option<&str>, at: &Value, out: &mut Vec<Found>) {
  let Some(text) = text else { return };
  if lazy_regex::regex_is_match!(r"\$listeners\b", text) {
    find(out, at, "vue/no-deprecated-dollar-listeners-api", "`$listeners` is gone in Vue 3; listeners are in `$attrs`".to_owned());
  }
  if lazy_regex::regex_is_match!(r"\$scopedSlots\b", text) {
    find(out, at, "vue/no-deprecated-dollar-scopedslots-api", "`$scopedSlots` is gone in Vue 3; use `$slots`".to_owned());
  }
}

/// An attribute name as the HTML parser and the compiler accept it.
fn valid_name(name: &str) -> bool {
  lazy_regex::regex_is_match!(r"^[A-Za-z_:@#][-A-Za-z0-9_:.@#]*$", name)
}

fn normal(exp: &str) -> String {
  exp.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The same attribute written twice, or written and bound: `class` and `style` may be both.
fn duplicates(node: &Value, out: &mut Vec<Found>) {
  let mut seen: Vec<String> = Vec::new();
  for p in props(node) {
    let name = match (p["prop"].as_str(), p["name"].as_str(), p["arg"].as_str()) {
      (Some("attribute"), Some(name), _) => name,
      (Some("directive"), Some("bind"), Some(arg)) if p["argStatic"] != false => arg,
      _ => continue,
    };
    let key = name.to_lowercase();
    if seen.contains(&key) && !matches!(key.as_str(), "class" | "style") {
      find(out, p, "vue/no-duplicate-attributes", format!("`{name}` is set twice"));
    }
    seen.push(key);
  }
}
