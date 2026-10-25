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

const PAGE: &str = r#"import { Embed } from "@snapfire/fsr/template";

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
  create(&root, NewOptions::default().fetch(false)).unwrap();
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

const LOADED: &str = r#"import { Embed } from "@snapfire/fsr/template";

export default function Page({ video, name, film }: { video: string; name: string; film: string }) {
  return (
    <main>
      <Embed provider="youtube" id={video} title={name} />
      <Embed provider="vimeo" id={film} title="A Vimeo film" />
    </main>
  );
}
"#;

const LOADER: &str = "export async function load() {\n  return { video: \"dQw4w9WgXcQ\", name: \"Never Gonna Give You Up\", film: \"76979871\" };\n}\n";

fn loaded(tag: &str, media: &str) -> PathBuf {
  let root = project(tag, LOADED, media);
  std::fs::write(root.join("app/routes/page.loader.ts"), LOADER).unwrap();
  root
}

#[test]
fn an_id_from_the_loader_renders_on_the_server_with_the_providers_still() {
  let root = loaded("loaded", "");
  let (html, policy) = render(&root);
  assert!(html.contains("data-sf-embed=\"https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1\"") && html.contains("data-sf-embed-title=\"Never Gonna Give You Up\""), "{html}");
  assert!(html.contains("src=\"https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg\""), "YouTube's still, named by the id: {html}");
  assert!(html.contains("data-sf-embed=\"https://player.vimeo.com/video/76979871?autoplay=1\""), "{html}");
  assert!(!html.contains("<iframe"), "no player before a click: {html}");
  for origin in ["https://i.ytimg.com", "https://www.youtube-nocookie.com", "https://player.vimeo.com"] {
    assert!(policy.contains(origin), "{origin}: {policy}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_computed_vimeo_id_shows_the_title_alone() {
  let root = loaded("vimeo", "");
  let (html, _) = render(&root);
  let vimeo = &html[html.find("player.vimeo.com").expect("the vimeo placeholder")..];
  let vimeo = &vimeo[..vimeo.find("</button>").unwrap()];
  assert!(!vimeo.contains("<img"), "Vimeo's still comes from its API, which the build cannot ask for an id it does not know: {vimeo}");
  assert!(vimeo.contains("A Vimeo film"), "{vimeo}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_computed_id_under_local_stills_falls_back_to_the_providers_still() {
  let root = loaded("loaded-local", "\n[media]\nembed_stills = \"local\"\n");
  let (html, policy) = render(&root);
  assert!(html.contains("src=\"https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg\""), "the build fetches only the stills it can name: {html}");
  assert!(policy.contains("https://i.ytimg.com"), "the fallback still's host is allowed: {policy}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn verbose_names_each_embed_whose_id_is_computed_under_local_stills() {
  let root = loaded("loaded-verbose", "\n[media]\nembed_stills = \"local\"\n");
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  let verbose = built.report.verbose().to_string();
  assert!(verbose.contains("embed     routes/page.tsx#default            youtube: the id is computed, so YouTube serves the still"), "{verbose}");
  assert!(verbose.contains("embed     routes/page.tsx#default            vimeo: the id is computed, so the placeholder shows the title alone"), "{verbose}");
  assert!(!built.report.to_string().contains("embed "), "{}", built.report);
  std::fs::remove_dir_all(&root).unwrap();

  let root = loaded("loaded-provider", "");
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  assert!(built.report.embeds.is_empty(), "the provider's still is what the configuration asked for: {:?}", built.report.embeds);
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_embed_the_build_cannot_place_is_refused() {
  let island = "import Player from \"@src/ui/Player\";\nexport default function Page() {\n  return <main><Player /></main>;\n}\n";
  for (tag, page, says) in [
    ("untitled", "import { Embed } from \"@snapfire/fsr/template\";\nexport default function Page() {\n  return <Embed provider=\"youtube\" id=\"x\" />;\n}\n", "needs a `title`"),
    ("unknown", "import { Embed } from \"@snapfire/fsr/template\";\nexport default function Page() {\n  return <Embed provider=\"tiktok\" id=\"x\" title=\"t\" />;\n}\n", "`youtube` or `vimeo`, not `tiktok`"),
    ("computed-provider", "import { Embed } from \"@snapfire/fsr/template\";\nexport default function Page({ p }: { p: string }) {\n  return <Embed provider={p} id=\"x\" title=\"t\" />;\n}\n", "an `<Embed>`'s `provider` is a string written out"),
    ("island", island, "places an `<Embed>` and hydrates"),
  ] {
    let root = project(tag, page, "");
    let app = root.join("app");
    if tag == "island" {
      std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr/client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
      std::fs::create_dir_all(app.join("src/ui")).unwrap();
      std::fs::write(app.join("src/ui/Player.tsx"), "import { useState } from \"react\";\nimport { Embed } from \"@snapfire/fsr/client/react\";\nexport default function Player() {\n  const [n, set] = useState(0);\n  return <div><button onClick={() => set(n + 1)}>{n}</button><Embed provider=\"youtube\" id=\"dQw4w9WgXcQ\" title=\"t\" /></div>;\n}\n").unwrap();
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
  let held = app.join("types/@snapfire/fsr/client/react.d.ts");
  std::fs::create_dir_all(held.parent().unwrap()).unwrap();
  std::fs::write(&held, "export {};\n").unwrap();
  let off = snapfire_fsr_cli::typecheck::Typecheck::default().enabled(false);
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
  let declared = std::fs::read_to_string(app.join("types/@snapfire/fsr/authoring/template.d.ts")).unwrap();
  assert!(declared.contains("export function Embed(props: EmbedProps): TemplateNode;"), "{declared}");
  std::fs::remove_dir_all(&root).unwrap();
}
