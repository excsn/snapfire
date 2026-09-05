use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use futures::StreamExt;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_service::MockTransport;
use snapfire_fsr_session::MemorySessionStore;

fn agent(id: i64, name: &str, region: &str, status: &str, queue: i64) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("name".to_owned(), Value::str(name));
  map.insert("region".to_owned(), Value::str(region));
  map.insert("status".to_owned(), Value::str(status));
  map.insert("queue_depth".to_owned(), Value::int(queue));
  map.insert("cpu".to_owned(), Value::F64(12.5));
  Value::Map(map)
}

fn alert(id: i64, agent_id: i64, level: &str, text: &str) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("agent_id".to_owned(), Value::int(agent_id));
  map.insert("level".to_owned(), Value::str(level));
  map.insert("text".to_owned(), Value::str(text));
  Value::Map(map)
}

fn job(id: i64, name: &str, seconds: i64) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::int(id));
  map.insert("name".to_owned(), Value::str(name));
  map.insert("seconds".to_owned(), Value::int(seconds));
  Value::Map(map)
}

fn fleet() -> Arc<MockTransport> {
  Arc::new(
    MockTransport::new()
      .returns("fleet.listAgents", Value::Seq(vec![agent(1, "builder-eu-1", "eu", "up", 3), agent(3, "builder-us-1", "us", "down", 7)]))
      .returns("fleet.getAgent", agent(1, "builder-eu-1", "eu", "up", 3))
      .returns("fleet.listJobs", Value::Seq(vec![job(11, "compile", 92)]))
      .returns("fleet.listAlerts", Value::Seq(vec![alert(21, 3, "page", "builder-us-1 stopped answering"), alert(22, 1, "warn", "queue over 3")]))
      .returns("fleet.acknowledgeAlert", Value::Seq(vec![alert(22, 1, "warn", "queue over 3")]))
      .returns("identity.authenticate", signed("alice", "admin")),
  )
}

fn signed(subject: &str, role: &str) -> Value {
  let mut claims = ValueMap::new();
  claims.insert("role".to_owned(), Value::str(role));
  let mut map = ValueMap::new();
  map.insert("subject".to_owned(), Value::str(subject));
  map.insert("claims".to_owned(), Value::Map(claims));
  map.insert("access_token".to_owned(), Value::Str(format!("svc-token-{subject}")));
  Value::Map(map)
}

/// The stock host over the mocks. Sessions live in the identity service in
/// `config/app.toml`; a canned transport cannot hold them, so these tests keep
/// them in memory and `tests/identity.rs` drives the service itself.
fn console(transport: Arc<MockTransport>) -> Host {
  let store = Arc::new(MemorySessionStore::new(64, std::time::Duration::from_secs(600)));
  let mut config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  if let Some(cache) = config.cache.as_mut() {
    cache.data = None;
  }
  Host::from_config(config).unwrap().services_over(transport).session_store(store).build().unwrap()
}

fn watching(session: &SessionCell, ids: &[i64]) {
  let mut map = ValueMap::new();
  for id in ids {
    map.insert(id.to_string(), Value::Bool(true));
  }
  session.insert("watching", Value::Map(map));
}

#[test]
fn two_layouts_seed_the_store_and_the_inner_one_wins_the_region() {
  let app = console(fleet());
  let home = block_on(app.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(home.contains("data-sf-store>{\"alerts/open\":{\"$\":\"f\",\"v\":2.0},\"fleet/watching\":{\"$\":\"f\",\"v\":0.0},\"fleet/region\":\"all\",\"ui/density\":\"comfortable\",\"fleet/headline\":\"2 to look at\"}</script>"), "{home}");
  assert!(home.contains("<p class=\"headline\">2 to look at</p>"), "the header renders the seeded headline: {home}");

  let session = SessionCell::default();
  watching(&session, &[1]);
  let eu = block_on(app.render_to_string("/agents?region=eu", RenderMode::Html, session)).unwrap();
  assert!(eu.contains("\"fleet/region\":\"eu\""), "the agents layout wins the key the root also sets: {eu}");
  assert!(eu.contains("\"fleet/headline\":\"2 to look at, watching 1\""), "{eu}");
  assert!(eu.contains("<span class=\"pill pill-region\">eu</span>"), "{eu}");
}

#[test]
fn a_document_streams_the_alerts_slot_and_the_agent_page_behind_their_own_fallbacks() {
  let app = console(fleet());
  let parts: Vec<String> = block_on(async { app.render("/agents/1", RenderMode::Html, SessionCell::default()).await.unwrap().collect().await });
  assert_eq!(parts.len(), 3, "the document, then two resolutions");
  assert!(parts[0].contains("data-sf-slot=\"1\"") && parts[0].contains("data-sf-slot=\"2\""), "{}", parts[0]);
  assert!(parts[0].contains("skeleton-title"), "the agent page's fallback: {}", parts[0]);
  let fills: Vec<&str> = parts[1..].iter().map(|p| if p.contains("class=\"alerts\"") { "alerts" } else { "agent" }).collect();
  assert!(fills.contains(&"alerts") && fills.contains(&"agent"), "{fills:?}");
  let agent = parts[1..].iter().find(|p| p.contains("class=\"page agent\"")).unwrap();
  assert!(agent.contains("<sf-s data-sf-island data-sf-when=\"visible\">"), "the job timeline is an island of its own: {agent}");
  assert!(agent.contains(";__sfHead({\"title\":\"builder-eu-1 · Ops console\""), "the page retitles the document on resolution: {agent}");
}

#[test]
fn both_slot_fallbacks_render_until_a_variant_fills_them() {
  let app = console(fleet());
  let html = block_on(app.render_to_string("/agents", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains("<sf-s data-sf-name=\"drawer\"><p class=\"drawer-hint\">Settings open here.</p></sf-s>"), "{html}");
  assert!(html.contains("<sf-s data-sf-name=\"peek\"><p class=\"peek-hint\">Peek at an agent without leaving the list.</p></sf-s>"), "{html}");
  assert!(html.contains("<div class=\"pick\">"), "the route's own page sits in the content slot: {html}");
}

#[test]
fn each_route_carries_a_variant_for_a_slot_of_a_different_layout() {
  let app = console(fleet());
  let (drawer, _) = app.intercept_for("/settings", Some("/"), None).expect("the root layout declares the drawer, and the summary sits under it");
  assert_eq!(drawer.children[0].1.children[0].0 .0, "drawer");
  assert!(app.intercept_for("/settings", Some("/agents"), None).is_some(), "so does the agent list");
  let (peek, _) = app.intercept_for("/agents/1", Some("/agents"), None).expect("the agents layout declares the peek slot");
  assert_eq!(peek.children[0].1.children[0].1.children[0].0 .0, "peek");
  assert!(app.intercept_for("/agents/1", Some("/"), None).is_none(), "the summary shares the root layout only, which declares no slot for this route");
  assert!(app.intercept_for("/agents/1", None, Some("peek")).is_some(), "`into` names the slot whatever the origin");
  assert!(app.intercept_for("/agents/1", None, Some("drawer")).is_none(), "a slot this route has no variant for");

  let payload = block_on(app.render_navigation_to_string("/agents/1", None, Some("peek"), SessionCell::default())).unwrap();
  let sidecar = payload.lines().find(|l| l.starts_with("G ")).unwrap();
  assert!(sidecar.contains("\"n\":\"peek\""), "{sidecar}");
  assert!(sidecar.contains("\"keep\":[\"content\"]"), "the agents layout keeps its page: {sidecar}");
  assert!(payload.contains("<h3>builder-eu-1</h3>"), "{payload}");
}

#[test]
fn acknowledging_an_alert_and_watching_an_agent_reach_the_backend_and_the_session() {
  let transport = fleet();
  let app = console(transport.clone());
  let session = SessionCell::default();

  let mut input = ValueMap::new();
  input.insert("alert_id".to_owned(), Value::int(21i64));
  let left = block_on(app.call_action("layout.alerts.ackAlert", session.clone(), Value::Map(input))).unwrap();
  assert_eq!(left, Value::Map(ValueMap::from_iter([("open".to_owned(), Value::Int(1))])));
  assert!(transport.calls().iter().any(|(method, _, _)| method == "fleet.acknowledgeAlert"));

  let mut input = ValueMap::new();
  input.insert("agent_id".to_owned(), Value::int(3i64));
  block_on(app.call_action("agents.watchAgent", session.clone(), Value::Map(input.clone()))).unwrap();
  assert_eq!(session.get("watching"), Some(Value::Map(ValueMap::from_iter([("3".to_owned(), Value::Bool(true))]))));
  let again = block_on(app.call_action("agents.watchAgent", session.clone(), Value::Map(input.clone())));
  assert!(again.is_err(), "watching twice is a conflict");

  let dropped = block_on(app.call_action("settings.unwatchAgent", session.clone(), Value::Map(input))).unwrap();
  assert_eq!(dropped, Value::Map(ValueMap::from_iter([("watching".to_owned(), Value::Int(0))])));
  assert_eq!(session.get("watching"), Some(Value::Map(ValueMap::new())));
  let mut input = ValueMap::new();
  input.insert("density".to_owned(), Value::str("compact"));
  block_on(app.call_action("settings.setDensity", session.clone(), Value::Map(input))).unwrap();
  assert_eq!(session.get("density"), Some(Value::str("compact")));
  let seeded = block_on(app.render_to_string("/", RenderMode::Html, session)).unwrap();
  assert!(seeded.contains("\"ui/density\":\"compact\""), "the setting comes back as a seed: {seeded}");
}

#[test]
fn the_handler_and_the_middleware_answer_before_any_page() {
  let app = console(fleet());
  let session = SessionCell::default();
  watching(&session, &[1, 3]);
  let Value::Map(got) = block_on(app.call_handler("GET", "/api/fleet", session, Value::Null)).unwrap() else { panic!("a map") };
  assert_eq!(got.get("open"), Some(&Value::int(2i64)));
  assert_eq!(got.get("watching"), Some(&Value::int(2i64)));

  use snapfire_fsr_host::{Preflight, PreflightAction};
  let redirect = block_on(app.preflight("GET", "/dashboard", SessionCell::default())).unwrap();
  assert_eq!(redirect.action, PreflightAction::Redirect { to: "/".into(), status: 307 });
  let rewrite = block_on(app.preflight("GET", "/fleet", SessionCell::default())).unwrap();
  assert_eq!(rewrite.action, PreflightAction::Rewrite("/agents".into()));
  let plain = block_on(app.preflight("GET", "/help", SessionCell::default())).unwrap();
  assert_eq!(plain, Preflight { action: PreflightAction::Continue, headers: vec![("x-ops-console".into(), "fsr".into())] });
}

#[test]
fn nothing_prerenders_under_a_layout_that_reads_the_session() {
  let app = console(fleet());
  assert!(app.report().app.prerenderable.is_empty(), "{:?}", app.report().app.prerenderable);
}

#[test]
fn a_prefixed_locale_renders_the_help_page_in_french_and_the_default_is_unprefixed() {
  let app = console(fleet());
  assert_eq!(app.report().locales, vec!["en_US".to_owned(), "fr_FR".to_owned()]);

  let french = block_on(app.render_to_string("/fr_FR/help", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(french.contains("<html lang=\"fr-FR\" data-sf-locale=\"fr_FR\">"), "{french}");
  assert!(french.contains("Comment ça marche"), "{french}");
  assert!(french.contains("<!--sf-g:routes/help/page.tsx#default@fr_FR-->"), "every segment key carries the locale: {french}");
  assert!(french.contains("<!--sf-g:routes/layout.tsx#default@fr_FR-->"), "{french}");

  let english = block_on(app.render_to_string("/help", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(english.contains("<html lang=\"en-US\" data-sf-locale=\"en_US\">"), "{english}");
  assert!(english.contains("How this works"), "{english}");
  assert!(english.contains("<!--sf-g:routes/help/page.tsx#default-->"), "the default locale leaves the key bare: {english}");

  let prefixed = block_on(app.render_to_string("/en_US/help", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(prefixed.contains("How this works"), "{prefixed}");
  assert!(prefixed.contains("<link rel=\"canonical\" href=\"/help\">"), "{prefixed}");

  let payload = block_on(app.render_to_string("/fr-fr/help", RenderMode::Payload, SessionCell::default())).unwrap();
  assert!(payload.contains("\nL \"fr_FR\"\n"), "{payload}");
  assert!(payload.contains("Comment ça marche"), "{payload}");
}

#[test]
fn the_edge_remembers_a_chosen_locale_and_an_action_takes_the_documents() {
  use bytes::Bytes;
  use http::{header, Request};
  let app = console(fleet());

  let response = block_on(app.handle(Request::get("/fr_FR/settings").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 200);
  let cookies: Vec<String> = response.headers().get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_owned()).collect();
  assert!(cookies.iter().any(|c| c.starts_with("sf_locale=fr_FR;")), "{cookies:?}");

  let response = block_on(app.handle(Request::get("/help").header(header::COOKIE, "sf_locale=fr_FR").body(Bytes::new()).unwrap()));
  let html = block_on(async { String::from_utf8(http_body_util::BodyExt::collect(response.into_body()).await.unwrap().to_bytes().to_vec()).unwrap() });
  assert!(html.contains("Comment ça marche"), "an unprefixed link keeps the chosen language: {html}");

  let response = block_on(app.handle(Request::get("/help").header(header::ACCEPT_LANGUAGE, "fr-CA, en;q=0.5").body(Bytes::new()).unwrap()));
  let html = block_on(async { String::from_utf8(http_body_util::BodyExt::collect(response.into_body()).await.unwrap().to_bytes().to_vec()).unwrap() });
  assert!(html.contains("Comment ça marche"), "the header's language: {html}");

  let session = SessionCell::default();
  watching(&session, &[]);
  let mut input = ValueMap::new();
  input.insert("density".to_owned(), Value::str("compact"));
  let value = block_on(app.call_action_in("settings.setDensity", session, app.locales().locale("fr_FR"), Value::Map(input))).unwrap();
  let Value::Map(map) = value else { panic!("a map") };
  assert_eq!(map.get("density"), Some(&Value::str("compact")));
}

#[test]
fn a_session_signs_in_through_the_host_and_its_fleet_call_carries_the_token() {
  use bytes::Bytes;
  use http::{header, Request};
  let transport = fleet();
  let app = console(transport.clone());
  assert_eq!(app.report().auth, Some(("service via identity".to_owned(), "/login".to_owned())));
  assert_eq!(app.report().bearer, vec![("fleet".to_owned(), "access_token".to_owned())]);
  let location = |response: &http::Response<snapfire_fsr_host::Body>| response.headers().get(header::LOCATION).unwrap().to_str().unwrap().to_owned();
  let text = |response: http::Response<snapfire_fsr_host::Body>| block_on(async { String::from_utf8(http_body_util::BodyExt::collect(response.into_body()).await.unwrap().to_bytes().to_vec()).unwrap() });

  let response = block_on(app.handle(Request::get("/account").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 307, "the middleware guards the account page");
  assert_eq!(location(&response), "/auth/login?return_to=/account");

  let response = block_on(app.handle(Request::get("/auth/login?return_to=/account").body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/login?return_to=%2Faccount");
  let cookie = response.headers().get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap()).find(|v| v.starts_with("sf_session=")).unwrap().split(';').next().unwrap().to_owned();

  let response = block_on(app.handle(Request::get("/login?return_to=%2Faccount").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 200);
  let html = text(response);
  assert!(html.contains("action=\"/auth/callback\""), "{html}");
  assert!(html.contains("Sign in") && !html.contains("Sign out"), "{html}");

  let response = block_on(app.handle(
    Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap(),
  ));
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/account");

  let response = block_on(app.handle(Request::get("/account").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 200);
  let html = text(response);
  assert!(html.contains("alice") && html.contains("admin"), "{html}");
  assert!(html.contains("Sign out"), "the header shows the session: {html}");
  assert_eq!(transport.last_metadata("authorization").as_deref(), Some("Bearer svc-token-alice"), "the loader's fleet call carried the token the identity service issued");
  let start = html.find("name=\"_csrf\" value=\"").map(|i| i + 20).expect("the sign-out form carries the token");
  let token = &html[start..start + html[start..].find('"').unwrap()];
  assert!(!token.is_empty());

  let response = block_on(app.handle(
    Request::post("/auth/logout").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from(format!("_csrf={token}"))).unwrap(),
  ));
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/");

  let response = block_on(app.handle(Request::get("/account").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()));
  assert_eq!(response.status(), 307, "signed out, the guard is back");
}
