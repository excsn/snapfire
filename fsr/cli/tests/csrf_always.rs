//! `[session] csrf = "always"` under each scheme: a page that renders the
//! token starts the session the token verifies against, and a page that does
//! not render it starts none, so its document stays cacheable.

use std::path::PathBuf;

use bytes::Bytes;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const SCHEMES: [&str; 3] = ["single_use", "session", "derived"];

const HOME: &str = "export default function Page() {\n  return <main>home</main>;\n}\n";

const SIGNUP: &str = r#"export default function Page({ csrf_token }: { csrf_token?: string }) {
  return (
    <form method="post" action="/_sf/action/signup.join">
      <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
      <button type="submit">Join</button>
    </form>
  );
}
"#;

const ACTIONS: &str = r#"import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";

export const join = action(async (_ctx: ActionCtx) => {
  return { ok: true };
});
"#;

fn project(scheme: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-csrf-{}-{n}-{scheme}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in [("routes/page.tsx", HOME), ("routes/signup/page.tsx", SIGNUP), ("routes/signup/actions.ts", ACTIONS)] {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[session]\n", &format!("[session]\ncsrf = \"always\"\ncsrf_scheme = \"{scheme}\"\n"), 1);
  std::fs::write(&config, text).unwrap();
  root
}

fn boot(root: &std::path::Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

struct Answer {
  status: u16,
  set_cookie: Option<String>,
  cache_control: Option<String>,
  location: Option<String>,
  body: String,
}

fn send(host: &Host, req: http::Request<Bytes>) -> Answer {
  tokio::runtime::Runtime::new().unwrap().block_on(async {
    use http_body_util::BodyExt as _;
    let response = host.handle(req).await;
    let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let status = response.status().as_u16();
    let set_cookie = response.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find(|v| v.starts_with("sf_session=")).map(str::to_owned);
    let cache_control = header("cache-control");
    let location = header("location");
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    Answer { status, set_cookie, cache_control, location, body }
  })
}

fn get(host: &Host, path: &str, cookie: Option<&str>) -> Answer {
  let mut req = http::Request::get(path);
  if let Some(cookie) = cookie {
    req = req.header("cookie", cookie);
  }
  send(host, req.body(Bytes::new()).unwrap())
}

fn token_in(html: &str) -> String {
  let at = html.find("name=\"_csrf\" value=\"").map(|i| i + 20).unwrap_or_else(|| panic!("a token field: {html}"));
  html[at..at + html[at..].find('"').unwrap()].to_owned()
}

#[test]
fn a_page_that_renders_no_token_starts_no_session() {
  let mut started = Vec::new();
  for scheme in SCHEMES {
    let root = project(scheme);
    let host = boot(&root);
    let answer = get(&host, "/", None);
    assert_eq!(answer.status, 200, "{scheme}: {}", answer.body);
    if answer.set_cookie.is_some() || answer.cache_control.as_deref() == Some("private, no-cache") {
      started.push(format!("{scheme}: {:?} {:?}", answer.set_cookie, answer.cache_control));
    }
    std::fs::remove_dir_all(&root).unwrap();
  }
  assert!(started.is_empty(), "a page with no form set a session cookie and went private under {started:#?}");
}

#[test]
fn a_page_that_renders_the_token_starts_the_session_it_verifies_against() {
  for scheme in SCHEMES {
    let root = project(scheme);
    let host = boot(&root);
    let answer = get(&host, "/signup", None);
    assert_eq!(answer.status, 200, "{scheme}: {}", answer.body);
    let token = token_in(&answer.body);
    assert!(!token.is_empty(), "{scheme}: an anonymous visitor is given a token");
    let set_cookie = answer.set_cookie.unwrap_or_else(|| panic!("{scheme}: the page that renders a token starts a session"));
    assert_eq!(answer.cache_control.as_deref(), Some("private, no-cache"), "{scheme}");
    let cookie = set_cookie.split(';').next().unwrap().to_owned();

    let post = http::Request::post("/_sf/action/signup.join")
      .header("cookie", &cookie)
      .header("content-type", "application/x-www-form-urlencoded")
      .header("referer", "http://localhost/signup")
      .body(Bytes::from(format!("_csrf={token}")))
      .unwrap();
    let answer = send(&host, post);
    assert_eq!(answer.status, 303, "{scheme}: the posted token verifies: {}", answer.body);
    assert_eq!(answer.location.as_deref(), Some("/signup"), "{scheme}");

    let answer = get(&host, "/", Some(&cookie));
    assert_eq!(answer.set_cookie, None, "{scheme}: a held session is not set again by a page with no form");
    std::fs::remove_dir_all(&root).unwrap();
  }
}
