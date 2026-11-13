use super::*;
use crate::ast::{Expr, Owner};

fn el(tag: &str, attrs: &[(&str, &str)], children: Vec<Tmpl>) -> Tmpl {
  Tmpl::Element { tag: tag.to_owned(), attrs: attrs.iter().map(|(name, value)| Entry::Field((*name).to_owned(), Expr::lit_str(*value))).collect(), children }
}

fn text(s: &str) -> Tmpl {
  Tmpl::Text(s.to_owned())
}

fn p(name: &str) -> Expr {
  Expr::var("$props").field(name)
}

fn component(owner: Owner, scope: Option<&str>, render: Tmpl) -> Component {
  Component { body: Vec::new(), render, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner, shadow: None, scope: scope.map(|id| crate::ast::ScopedStyle { id: id.to_owned(), slotted: true }) }
}

/// A page the way MDX lowers one: highlighted code whose text holds what the
/// markup rules escape differently, adjacent text, void and SVG elements,
/// dynamic text, a branch, a loop and a placement given literal children.
fn page() -> Tmpl {
  let line = el("span", &[("data-line", "")], vec![el("span", &[("class", "hl-k")], vec![text("fn")]), text(" "), el("span", &[("class", "hl-n")], vec![text("main")]), text("() -> Vec<u8> { 'a' \"b\" & c }")]);
  let adjacent = el("span", &[("data-line", "")], vec![text("x"), text("y"), text("it's")]);
  let code = el("figure", &[("data-rehype-pretty-code-figure", "")], vec![el("pre", &[("class", "hl-code"), ("data-language", "rust")], vec![el("code", &[("data-language", "rust")], vec![line, text("\n"), adjacent])])]);
  let prose = el("p", &[], vec![text("it's "), Tmpl::Expr(p("name")), text(" & more")]);
  let media = el("div", &[("class", "media")], vec![el("br", &[], Vec::new()), el("img", &[("src", "a.png"), ("alt", "a")], Vec::new()), el("svg", &[("viewBox", "0 0 1 1")], vec![el("path", &[("d", "M0 0")], Vec::new())])]);
  let branch = Tmpl::If { cond: p("on"), then: Box::new(el("section", &[], vec![el("h2", &[], vec![text("Title > sub")])])), r#else: Some(Box::new(text("off"))) };
  let each = Tmpl::For { over: p("items"), params: vec!["item".to_owned()], body: Box::new(el("li", &[("class", "item")], vec![text("item: "), Tmpl::Expr(Expr::var("item"))])) };
  let placed = Tmpl::Component { module: "Inner".to_owned(), props: Vec::new(), children: vec![el("em", &[], vec![text("slotted "), text("text")])], id: 0, keyed: false };
  Tmpl::Fragment(vec![code, prose, media, branch, each, placed])
}

/// A component whose root is an element, which takes its caller's stamps.
fn rooted() -> Tmpl {
  el("article", &[("data-v-own", "")], vec![el("h1", &[("data-v-own", "")], vec![text("Hi")]), el("p", &[("data-v-own", "")], vec![text("x > y")])])
}

/// What a Vue caller hands a placement: its scope, which the placed
/// component's root takes as a stamp, plus attributes that fall through to it.
fn rooted_props() -> Vec<Entry> {
  vec![
    Entry::Field(PARENT_SCOPE_PROP.to_owned(), Expr::lit_str("data-v-caller")),
    Entry::Field(FALLTHROUGH_PROP.to_owned(), Expr::Object(vec![Entry::Field("class".to_owned(), Expr::lit_str("fell"))])),
  ]
}

fn props() -> ValueMap {
  [("name", Value::str("Ann")), ("on", Value::Bool(true)), ("items", Value::Seq(vec![Value::str("a"), Value::str("b")].into()))].into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn frameworks() -> Vec<Frameworks> {
  let mut out = vec![Frameworks::default()];
  for react in ReactMajor::ALL {
    out.push(Frameworks::default().react(Some(react)).vue(Some(VueMajor::V3)).svelte(Some(SvelteMajor::V5)));
  }
  out
}

fn render_with(frameworks: Frameworks, caller: &Component, library: &Components) -> String {
  Interpreter::default().with_frameworks(frameworks).render(caller, &props(), library).unwrap().html
}

#[test]
fn prebuilt_markup_renders_byte_for_byte_as_the_tree_did_under_every_owner_and_framework() {
  let owners = [Owner::Fsr, Owner::React, Owner::Vue, Owner::Svelte];
  for frameworks in frameworks() {
    for owner in owners {
      for caller_owner in owners {
        let inner = component(owner, None, el("div", &[("class", "inner")], vec![Tmpl::Slot("content".to_owned())]));
        let child = component(owner, Some("data-v-own"), page());
        let root_child = component(owner, Some("data-v-own"), rooted());
        let caller = component(caller_owner, Some("data-v-caller"), Tmpl::Fragment(vec![
          Tmpl::Component { module: "Page".to_owned(), props: vec![Entry::Spread(Expr::var("$props"))], children: Vec::new(), id: 0, keyed: false },
          Tmpl::Component { module: "Rooted".to_owned(), props: rooted_props(), children: Vec::new(), id: 1, keyed: false },
        ]));
        let library = |page: Component, rooted: Component| -> Components {
          [("Inner", inner.clone()), ("Page", page), ("Rooted", rooted)].into_iter().map(|(k, v)| (k.to_owned(), Arc::new(v))).collect()
        };
        let before = render_with(frameworks, &caller, &library(child.clone(), root_child.clone()));
        let (mut baked, mut baked_root) = (child.clone(), root_child.clone());
        bake_static_markup(&mut baked);
        bake_static_markup(&mut baked_root);
        let after = render_with(frameworks, &caller, &library(baked, baked_root));
        assert_eq!(after, before, "owner {owner:?} under {caller_owner:?} with {frameworks:?}");
      }
    }
  }
}

fn markers(t: &Tmpl) -> usize {
  match t {
    Tmpl::Marker(_) => 1,
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Component { children, .. } => children.iter().map(markers).sum(),
    Tmpl::If { then, r#else, .. } => markers(then) + r#else.as_deref().map_or(0, markers),
    Tmpl::For { body, .. } => markers(body),
    _ => 0,
  }
}

#[test]
fn literal_markup_is_prebuilt_while_a_root_element_and_a_placements_children_stay_nodes() {
  let mut baked = component(Owner::Fsr, None, page());
  bake_static_markup(&mut baked);
  let Tmpl::Fragment(children) = &baked.render else { panic!("the page stays a fragment") };
  assert!(markers(&baked.render) >= 4, "{:?}", baked.render);
  assert!(matches!(&children[0], Tmpl::Marker(markup) if markup.starts_with("<figure data-rehype-pretty-code-figure=\"\"><pre class=\"hl-code\"")), "{:?}", children[0]);
  let placed = children.iter().find_map(|child| match child {
    Tmpl::Component { children, .. } => Some(children),
    _ => None,
  });
  assert_eq!(placed.expect("the placement stays"), &vec![el("em", &[], vec![text("slotted "), text("text")])]);
  let mut root = component(Owner::Vue, Some("data-v-own"), rooted());
  bake_static_markup(&mut root);
  assert!(matches!(&root.render, Tmpl::Element { tag, .. } if tag == "article"), "{:?}", root.render);
}


#[test]
fn an_element_the_runtime_reads_from_the_plan_stays_a_node() {
  let page = Tmpl::Fragment(vec![el("div", &[("class", "wrap")], vec![
    el("style", &[], vec![text(".a { color: red }")]),
    el("video", &[("src", "/clip.mp4")], vec![text("no video")]),
    el("div", &[("data-sf-embed", "https://www.youtube.com/embed/x")], vec![text("play")]),
    el("p", &[], vec![text("prose")]),
  ])]);
  let mut baked = component(Owner::Fsr, None, page);
  bake_static_markup(&mut baked);
  assert_eq!(crate::render::inline_styles(&baked), vec![Some(".a { color: red }".to_owned())], "{:?}", baked.render);
  assert_eq!(crate::ast::embed_players(&baked.render), vec!["https://www.youtube.com/embed/x".to_owned()], "{:?}", baked.render);
  assert_eq!(crate::ast::media_sources(&baked.render), vec!["/clip.mp4".to_owned()], "{:?}", baked.render);
  assert!(markers(&baked.render) >= 1, "the rest is still prebuilt: {:?}", baked.render);
}
