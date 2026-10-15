//! `<Embed>`: a still and a play button the server renders, the player only
//! on a click. The still is the provider's unless `[media] embed_stills` is
//! `local`, and the policy names what the page reaches.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = r#"import { Embed } from "@snapfire/fsr-authoring/template";

export default function Page() {
  return (
    <main>
      <Embed provider="youtube" id="dQw4w9WgXcQ" title="Never Gonna Give You Up" className="clip" />
      <Embed provider="vimeo" id="76979871" title="A Vimeo film" />
    </main>
  );
}
"#;

fn project(tag: &str, page: &str, media: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-embeds-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("routes/page.tsx"), page).unwrap();
  let embeds = app.join("generated/embeds");
  std::fs::create_dir_all(&embeds).unwrap();
  std::fs::write(embeds.join("vimeo-76979871.json"), r#"[{"id":76979871,"thumbnail_large":"https://i.vimeocdn.com/video/452001751-640.jpg","width":1280,"height":720}]"#).unwrap();
  std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/noticeboard_tera/app/images/board.png"), embeds.join("youtube-dQw4w9WgXcQ.jpg")).unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[document.csp]\ndefault-src = [\"'self'\"]\n{media}")).unwrap();
  root
}

fn render(root: &PathBuf) -> (String, String) {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let html = tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  (html, host.report().csp.clone().expect("a policy"))
}

#[test]
fn by_default_the_still_is_the_providers_and_the_player_waits_for_a_click() {
  let root = project("provider", PAGE, "");
  let (html, policy) = render(&root);
  assert!(html.contains("data-sf-embed=\"https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1\"") && html.contains("data-sf-embed-title=\"Never Gonna Give You Up\""), "{html}");
  assert!(html.contains("src=\"https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg\""), "YouTube's own still: {html}");
  assert!(html.contains("src=\"https://i.vimeocdn.com/video/452001751-640.jpg\"") && html.contains("width=\"640\""), "Vimeo's, from its video API: {html}");
  assert!(!html.contains("<iframe"), "no player before a click: {html}");
  assert!(html.contains("<button type=\"button\"") && html.contains("class=\"clip\""), "{html}");
  for origin in ["img-src", "https://i.ytimg.com", "https://i.vimeocdn.com", "frame-src", "https://www.youtube-nocookie.com", "https://player.vimeo.com"] {
    assert!(policy.contains(origin), "{origin}: {policy}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_local_still_is_served_from_the_application_and_the_policy_names_no_image_host() {
  let root = project("local", PAGE, "\n[media]\nembed_stills = \"local\"\n");
  let (html, policy) = render(&root);
  assert!(html.contains("src=\"/static/js/app/generated/embeds/youtube-dQw4w9WgXcQ."), "the fetched still, hashed: {html}");
  assert!(!html.contains("i.ytimg.com"), "{html}");
  assert!(!policy.contains("i.ytimg.com") && policy.contains("https://www.youtube-nocookie.com"), "the player still loads from the provider on a click: {policy}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_embed_the_build_cannot_place_is_refused() {
  let island = "import Player from \"@src/ui/Player\";\nexport default function Page() {\n  return <main><Player /></main>;\n}\n";
  for (tag, page, says) in [
    ("untitled", "import { Embed } from \"@snapfire/fsr-authoring/template\";\nexport default function Page() {\n  return <Embed provider=\"youtube\" id=\"x\" />;\n}\n", "needs a `title`"),
    ("unknown", "import { Embed } from \"@snapfire/fsr-authoring/template\";\nexport default function Page() {\n  return <Embed provider=\"tiktok\" id=\"x\" title=\"t\" />;\n}\n", "`youtube` or `vimeo`, not `tiktok`"),
    ("island", island, "places an `<Embed>` and hydrates"),
  ] {
    let root = project(tag, page, "");
    let app = root.join("app");
    if tag == "island" {
      std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
      std::fs::create_dir_all(app.join("src/ui")).unwrap();
      std::fs::write(app.join("src/ui/Player.tsx"), "import { useState } from \"react\";\nimport { Embed } from \"@snapfire/fsr-client/react\";\nexport default function Player() {\n  const [n, set] = useState(0);\n  return <div><button onClick={() => set(n + 1)}>{n}</button><Embed provider=\"youtube\" id=\"dQw4w9WgXcQ\" title=\"t\" /></div>;\n}\n").unwrap();
    }
    let error = build(&app, &Options::beside(&app)).err().unwrap_or_else(|| panic!("{tag}: refused")).to_string();
    assert!(error.contains(says), "{tag}: {error}");
    std::fs::remove_dir_all(&root).unwrap();
  }
}

#[test]
fn a_check_typechecks_against_the_client_this_binary_carries() {
  let root = project("stale", "export default function Page() {\n  return <main />;\n}\n", "");
  let app = root.join("app");
  let held = app.join("types/@snapfire/fsr-client/react.d.ts");
  std::fs::create_dir_all(held.parent().unwrap()).unwrap();
  std::fs::write(&held, "export {};\n").unwrap();
  let off = snapfire_fsr_cli::typecheck::Typecheck { enabled: false, ..Default::default() };
  snapfire_fsr_cli::check(&app, &Options::beside(&app), &off, |_| {}).unwrap_or_else(|e| panic!("checks: {e}"));
  assert!(std::fs::read_to_string(&held).unwrap().contains("export declare function Embed"), "the stale declarations are replaced before the checker reads them");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_bare_application_placing_an_embed_typechecks() {
  let root = project("typed", PAGE, "[media]\nembed_stills = \"local\"\n\n[typecheck]\nversion = \"7.0.2\"\n");
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let (_, checked) = snapfire_fsr_cli::check(&app, &Options::beside(&app), &snapfire_fsr_cli::typecheck::Typecheck::beside(&app), |_| {}).unwrap_or_else(|e| panic!("checks: {e}"));
  let checked = checked.expect("the checker ran");
  let errors: Vec<String> = checked.diagnostics.iter().map(|d| d.to_string()).collect();
  assert!(errors.is_empty(), "{errors:?}");
  let declared = std::fs::read_to_string(app.join("types/@snapfire/fsr-authoring/template.d.ts")).unwrap();
  assert!(declared.contains("export function Embed(props: EmbedProps): TemplateNode;"), "{declared}");
  std::fs::remove_dir_all(&root).unwrap();
}
