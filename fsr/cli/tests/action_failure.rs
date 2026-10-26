//! A page reading `action_failure` shows the failure of the action that sent
//! the visitor back to it, once, with a render cache configured as a new
//! project's is. The failure rides in the session for the one request after
//! the redirect, so a render shared with any other request would hide it or
//! show it again.

use std::path::PathBuf;

use bytes::Bytes;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const FORM_PAGE: &str = r#"export default function Page({ csrf_token, action_failure }: { csrf_token?: string; action_failure?: { kind: string; message: string } }) {
  return (
    <main>
      {action_failure ? <p className="failure">{action_failure.message}</p> : <p className="quiet">nothing failed</p>}
      <form method="post" action="/_sf/action/join.take">
        <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
        <button type="submit">Take</button>
      </form>
    </main>
  );
}
"#;

const BARE_PAGE: &str = r#"export default function Page({ action_failure }: { action_failure?: { kind: string; message: string } }) {
  return <main>{action_failure ? <p className="failure">{action_failure.message}</p> : <p className="quiet">nothing failed</p>}</main>;
}
"#;

const ACTIONS: &str = r#"import { action, fail } from "@snapfire/fsr/host";
import type { ActionCtx } from "@snapfire/fsr/host";

export const take = action(async (_ctx: ActionCtx) => {
  fail("invalid", "that name is taken");
  return { ok: true };
});
"#;

fn project(cache: bool) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-failure-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in [("routes/join/page.tsx", FORM_PAGE), ("routes/join/actions.ts", ACTIONS), ("routes/bare/page.tsx", BARE_PAGE)] {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[session]\n", "[session]\ncsrf = \"always\"\ncsrf_scheme = \"session\"\n", 1);
  assert!(text.contains("[cache]"), "a new project carries a render cache: {text}");
  let text = if cache { text } else { text.replace("[cache]", "[cache_off]") };
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
  body: String,
}

fn send(host: &Host, req: http::Request<Bytes>) -> Answer {
  tokio::runtime::Runtime::new().unwrap().block_on(async {
    use http_body_util::BodyExt as _;
    let response = host.handle(req).await;
    let status = response.status().as_u16();
    let set_cookie = response.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find(|v| v.starts_with("sf_session=")).map(|v| v.split(';').next().unwrap().to_owned());
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    Answer { status, set_cookie, body }
  })
}

fn get(host: &Host, path: &str, cookie: &str) -> String {
  send(host, http::Request::get(path).header("cookie", cookie).body(Bytes::new()).unwrap()).body
}

fn token_in(html: &str) -> String {
  let at = html.find("name=\"_csrf\" value=\"").map(|i| i + 20).unwrap_or_else(|| panic!("a token field: {html}"));
  html[at..at + html[at..].find('"').unwrap()].to_owned()
}

#[test]
fn a_page_shows_the_failure_of_the_action_that_sent_the_visitor_back_once() {
  for cache in [false, true] {
    shows_the_failure_once(cache);
  }
}

fn shows_the_failure_once(cache: bool) {
  let root = project(cache);
  let host = boot(&root);
  let first = send(&host, http::Request::get("/join").body(Bytes::new()).unwrap());
  assert_eq!(first.status, 200, "{}", first.body);
  let cookie = first.set_cookie.expect("the page that renders a token starts a session");
  let token = token_in(&first.body);

  for page in ["/join", "/bare"] {
    for _ in 0..2 {
      let html = get(&host, page, &cookie);
      assert!(html.contains("nothing failed"), "cache {cache}, {page} before the post: {html}");
    }
    let post = http::Request::post("/_sf/action/join.take")
      .header("cookie", &cookie)
      .header("content-type", "application/x-www-form-urlencoded")
      .header("referer", format!("http://localhost{page}"))
      .body(Bytes::from(format!("_csrf={token}")))
      .unwrap();
    let answer = send(&host, post);
    assert_eq!(answer.status, 303, "cache {cache}, {page}: a browser's post is answered with its page: {}", answer.body);

    let html = get(&host, page, &cookie);
    assert!(html.contains("that name is taken"), "cache {cache}, {page}: the request after the redirect shows the failure: {html}");
    let html = get(&host, page, &cookie);
    assert!(html.contains("nothing failed"), "cache {cache}, {page}: the next request does not show it again: {html}");
  }
  let _ = std::fs::remove_dir_all(&root);
}
