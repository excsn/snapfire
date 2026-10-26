//! A page reading `csp_nonce` renders the nonce of the response it is in. The
//! browser runs an inline script only when its nonce is the one that
//! response's policy names, so a render carried from an earlier response is a
//! script that never runs.

use std::path::PathBuf;

use bytes::Bytes;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = r#"export default function Page({ csp_nonce }: { csp_nonce?: string }) {
  return (
    <main>
      <p id="opened"></p>
      <script nonce={csp_nonce}>{"document.getElementById('opened').textContent = 'opened';"}</script>
    </main>
  );
}
"#;

const LOADED_PAGE: &str = r#"export default function Page({ count, csp_nonce }: { count: number; csp_nonce?: string }) {
  return (
    <main>
      <p>{count} reserved</p>
      <script nonce={csp_nonce}>{"window.opened = true;"}</script>
    </main>
  );
}
"#;

const LOADER: &str = r#"export async function load() {
  return { count: 2 };
}
"#;

fn project() -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-nonce-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in [("routes/plain/page.tsx", PAGE), ("routes/loaded/page.tsx", LOADED_PAGE), ("routes/loaded/page.loader.ts", LOADER)] {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[document.csp]\nscript-src = [\"'self'\"]\n")).unwrap();
  root
}

fn boot(root: &std::path::Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

/// The policy's nonce and every nonce the document's scripts carry.
fn nonces(host: &Host, path: &str) -> (String, Vec<String>) {
  tokio::runtime::Runtime::new().unwrap().block_on(async {
    use http_body_util::BodyExt as _;
    let response = host.handle(http::Request::get(path).body(Bytes::new()).unwrap()).await;
    let policy = response.headers().get("content-security-policy").and_then(|v| v.to_str().ok()).expect("a policy").to_owned();
    let at = policy.find("'nonce-").expect("the policy names a nonce") + "'nonce-".len();
    let named = policy[at..at + policy[at..].find('\'').unwrap()].to_owned();
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    let carried = body.split("nonce=\"").skip(1).map(|rest| rest[..rest.find('"').unwrap()].to_owned()).collect();
    (named, carried)
  })
}

#[test]
fn every_response_renders_the_nonce_its_policy_names() {
  let root = project();
  let host = boot(&root);
  for path in ["/plain", "/loaded"] {
    let mut seen = Vec::new();
    for _ in 0..3 {
      let (named, carried) = nonces(&host, path);
      assert!(!carried.is_empty(), "{path}: the page's script carries a nonce");
      for nonce in &carried {
        assert_eq!(nonce, &named, "{path}: a script carries a nonce its response's policy does not name");
      }
      seen.push(named);
    }
    seen.dedup();
    assert_eq!(seen.len(), 3, "{path}: each response its own nonce");
  }
  let _ = std::fs::remove_dir_all(&root);
}
