//! `--desktop`: the address the window opens and the one `fsr serve` binds for it.

use snapfire_fsr_cli::desktop;
use snapfire_fsr_cli::serve::ServeOptions;

#[test]
fn a_wildcard_listen_address_opens_on_loopback() {
  assert_eq!(desktop::url("0.0.0.0:8140", false).unwrap(), "http://127.0.0.1:8140/");
  assert_eq!(desktop::url("[::]:8140", false).unwrap(), "http://[::1]:8140/");
}

#[test]
fn a_loopback_address_opens_as_given_and_tls_opens_https() {
  assert_eq!(desktop::url("127.0.0.1:9000", false).unwrap(), "http://127.0.0.1:9000/");
  assert_eq!(desktop::url("[::1]:9000", true).unwrap(), "https://[::1]:9000/");
  assert_eq!(desktop::url("localhost:9000", false).unwrap(), "http://localhost:9000/");
}

#[test]
fn a_listen_address_the_window_cannot_open_is_refused() {
  assert!(desktop::url("127.0.0.1:0", false).is_err());
  assert!(desktop::url("nowhere", false).is_err());
}

#[test]
fn serve_for_a_window_binds_an_ephemeral_loopback_port_unless_given_one() {
  assert_eq!(ServeOptions::default().desktop(true).address("0.0.0.0:8140"), "127.0.0.1:0");
  assert_eq!(ServeOptions::default().desktop(true).listen(Some("127.0.0.1:9000".to_owned())).address("0.0.0.0:8140"), "127.0.0.1:9000");
  assert_eq!(ServeOptions::default().address("0.0.0.0:8140"), "0.0.0.0:8140");
}

#[test]
fn the_window_title_leads_with_the_app_name_then_the_page_title() {
  assert_eq!(desktop::window_title("Uploads", "Tour"), "Uploads - Tour");
  assert_eq!(desktop::window_title("Uploads", "Uploads"), "Uploads");
  assert_eq!(desktop::window_title("Uploads", " "), "Uploads");
}

#[test]
fn the_app_name_is_the_configured_one_else_the_project_directory() {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let project = std::env::temp_dir().join(format!("fsr-desktop-{}-{nanos}", std::process::id())).join("storefront");
  std::fs::create_dir_all(project.join("app")).unwrap();
  let toml = "[app]\ndir = \"app\"\n\n[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"k\"\n";
  std::fs::write(project.join("app.toml"), toml).unwrap();
  assert_eq!(desktop::name(&project.join("app")), "storefront");
  std::fs::write(project.join("app.toml"), format!("{toml}\n[desktop]\nname = \"Uploads\"\n")).unwrap();
  assert_eq!(desktop::name(&project.join("app")), "Uploads");
}

#[test]
fn every_menu_shortcut_on_every_platform_parses() {
  use desktop::{Command, Platform};
  for platform in [Platform::MacOs, Platform::Windows, Platform::Linux] {
    for command in Command::ALL {
      if let Some(keys) = desktop::shortcut(command, platform) {
        assert!(keys.parse::<muda::accelerator::Accelerator>().is_ok(), "{platform:?} {command:?}: `{keys}` does not parse");
      }
    }
  }
}

#[test]
fn back_and_forward_walk_the_page_history_reload_reloads_and_quit_closes() {
  use desktop::{Command, Effect};
  assert_eq!(desktop::effect(Command::Back), Effect::Script("history.back()"));
  assert_eq!(desktop::effect(Command::Forward), Effect::Script("history.forward()"));
  assert_eq!(desktop::effect(Command::Reload), Effect::Reload);
  assert_eq!(desktop::effect(Command::Quit), Effect::Close);
}

#[test]
fn macos_leads_with_the_app_menu_and_windows_and_linux_with_file() {
  use desktop::Platform;
  let titles = |platform| desktop::menus(platform, "Uploads").iter().map(|menu| menu.title.clone()).collect::<Vec<_>>();
  assert_eq!(titles(Platform::MacOs), ["Uploads", "Edit", "History"]);
  assert_eq!(titles(Platform::Windows), ["File", "Edit", "History"]);
  assert_eq!(titles(Platform::Linux), ["File", "Edit", "History"]);
}

#[test]
fn the_jar_sends_back_what_the_host_set_until_it_expires_or_is_cleared() {
  let mut jar = desktop::CookieJar::default();
  jar.store("sf_session=abc; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600", 1_000);
  jar.store("sf_locale=fr_FR; Path=/; Max-Age=31536000; SameSite=Lax", 1_000);
  assert_eq!(jar.header(1_000).as_deref(), Some("sf_session=abc; sf_locale=fr_FR"));
  assert_eq!(jar.header(4_601).as_deref(), Some("sf_locale=fr_FR"), "the session's Max-Age ran out");
  jar.store("sf_session=xyz; Path=/; HttpOnly; Max-Age=60", 1_000);
  jar.store("sf_locale=; Path=/; Max-Age=0", 1_000);
  assert_eq!(jar.header(1_000).as_deref(), Some("sf_session=xyz"), "a value replaces its name and Max-Age=0 clears it");
  jar.store("sf_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0", 1_000);
  assert_eq!(jar.header(1_000), None);
}

#[test]
fn a_jar_on_disk_keeps_what_has_not_expired_and_no_cookie_without_an_end() {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let file = std::env::temp_dir().join(format!("fsr-jar-{}-{nanos}", std::process::id())).join("cookies.json");
  let mut jar = desktop::CookieJar::default();
  jar.store("sf_session=abc; Path=/; HttpOnly; Max-Age=3600", 1_000);
  jar.store("short=1; Max-Age=10", 1_000);
  jar.store("run=1; Path=/", 1_000);
  jar.save(&file).unwrap();
  let read = desktop::CookieJar::load(&file, 1_100);
  assert_eq!(read.header(1_100).as_deref(), Some("sf_session=abc"));
  assert_eq!(desktop::CookieJar::load(&file.with_file_name("absent.json"), 1_100).header(1_100), None);
}

#[test]
fn a_scheme_request_reaches_the_host_as_a_path_on_localhost_carrying_the_jar() {
  let request = http::Request::post("app://localhost/_sf/action/cart.add?x=1").header("content-type", "text/plain").body(b"hello".to_vec()).unwrap();
  let request = desktop::to_host(request, Some("sf_session=abc".to_owned()));
  assert_eq!(request.uri().to_string(), "/_sf/action/cart.add?x=1");
  assert_eq!(request.headers()["host"], "localhost");
  assert_eq!(request.headers()["cookie"], "sf_session=abc");
  assert_eq!(request.headers()["content-type"], "text/plain");
  assert_eq!(&request.body()[..], b"hello");
}

#[test]
fn the_cookie_file_lives_in_each_platforms_data_directory() {
  use desktop::Platform;
  use std::path::{Path, PathBuf};
  let home = Path::new("/home/u");
  assert_eq!(desktop::data_dir(Platform::MacOs, "Uploads", home, None, None), PathBuf::from("/home/u/Library/Application Support/Uploads"));
  assert_eq!(desktop::data_dir(Platform::Windows, "Uploads", home, Some(Path::new("C:/AppData")), None), PathBuf::from("C:/AppData/Uploads"));
  assert_eq!(desktop::data_dir(Platform::Linux, "Uploads", home, None, None), PathBuf::from("/home/u/.local/share/Uploads"));
  assert_eq!(desktop::data_dir(Platform::Linux, "Uploads", home, None, Some(Path::new("/xdg"))), PathBuf::from("/xdg/Uploads"));
}

#[test]
fn the_scheme_limits_name_every_route_that_streams() {
  let manifest = snapfire_fsr_plan::Manifest::from_json(
    r#"{ "version": 2, "routes": [
      { "pattern": "/", "plan": { "id": 0, "module": "routes/page.tsx#default" } },
      { "pattern": "/cart", "plan": { "id": 0, "module": "shell#document", "children": [
        { "slot": "content", "node": { "id": 1, "module": "routes/cart/page.tsx#default", "deferred": true, "fallback": "routes/cart/loading.tsx#default" } } ] } } ] }"#,
  )
  .unwrap();
  let limits = desktop::scheme_limits(&manifest);
  assert!(limits.iter().any(|line| line.contains("/cart") && line.contains("loading")), "{limits:?}");
  assert!(!limits.iter().any(|line| line.contains("`/`")), "{limits:?}");
  assert!(limits.iter().any(|line| line.contains("live")), "{limits:?}");
}

#[test]
fn a_redirect_answering_a_navigation_reaches_the_window_as_a_page_that_replaces_itself() {
  let redirect = || http::Response::builder().status(303).header("location", "/?q=\"x\"&a=<b>").header("set-cookie", "sf_state=1").body(Vec::new()).unwrap();
  let page = desktop::followable(redirect(), true);
  assert_eq!(page.status(), 200);
  assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
  assert!(page.headers().get("location").is_none());
  assert_eq!(page.headers()["set-cookie"], "sf_state=1");
  let body = String::from_utf8(page.into_body()).unwrap();
  assert!(body.contains(r#"<meta http-equiv="refresh" content="0;url=/?q=&quot;x&quot;&amp;a=&lt;b&gt;">"#), "{body}");
  assert!(body.contains(r#"location.replace("/?q=\"x\"&a=\u003cb>")"#), "{body}");

  assert_eq!(desktop::followable(redirect(), false).status(), 303, "a fetch reads the redirect itself");
  let ok = http::Response::builder().status(200).body(b"page".to_vec()).unwrap();
  assert_eq!(desktop::followable(ok, true).into_body(), b"page");
}

#[test]
fn a_stream_is_refused_with_no_content_so_the_page_stops_asking() {
  let stream = http::HeaderMap::from_iter([(http::header::CONTENT_TYPE, http::HeaderValue::from_static("text/event-stream"))]);
  assert!(desktop::streams(&stream));
  let page = http::HeaderMap::from_iter([(http::header::CONTENT_TYPE, http::HeaderValue::from_static("text/html; charset=utf-8"))]);
  assert!(!desktop::streams(&page));
  assert!(!desktop::streams(&http::HeaderMap::new()));
}
