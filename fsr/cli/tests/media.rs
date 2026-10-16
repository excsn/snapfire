//! A video, an audio file or a caption track a component imports: served as
//! the file it is under its hash, its URL in the markup, and another origin a
//! media element names added to `media-src`.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

const PAGE: &str = r#"import clip from "../src/media/clip.mp4";
import captions from "../src/media/clip.vtt";

export default function Page() {
  return (
    <main>
      <video src={clip} controls preload="metadata" width={320} height={180}>
        <track kind="captions" src={captions} srcLang="en" label="English" />
      </video>
      <audio controls src="https://sounds.example/theme.mp3" />
    </main>
  );
}
"#;

fn project() -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-media-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("routes/page.tsx"), PAGE).unwrap();
  std::fs::create_dir_all(app.join("src/media")).unwrap();
  std::fs::write(app.join("src/media/clip.mp4"), b"stands in for an mp4").unwrap();
  std::fs::write(app.join("src/media/clip.vtt"), "WEBVTT\n\n00:00.000 --> 00:01.000\nHello\n").unwrap();
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  std::fs::write(&config, format!("{text}\n[document.csp]\ndefault-src = [\"'self'\"]\n")).unwrap();
  root
}

#[test]
fn an_imported_clip_and_its_captions_are_served_under_their_hash() {
  let root = project();
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  let clip = built.assets.media.iter().find(|m| m.source == "src/media/clip.mp4").expect("the clip is in the manifest");
  let captions = built.assets.media.iter().find(|m| m.source == "src/media/clip.vtt").expect("the captions are in the manifest");
  assert!(clip.path.starts_with("src/media/clip.") && clip.path.ends_with(".mp4") && clip.url.ends_with(&clip.path), "{clip:?}");
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(&root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let html = tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(html.contains(&format!("<video src=\"{}\"", clip.url)), "the server writes the hashed URL: {html}");
  assert!(html.contains(&format!("src=\"{}\"", captions.url)), "{html}");
  let policy = host.report().csp.clone().expect("a policy");
  assert!(policy.contains("media-src") && policy.contains("https://sounds.example"), "another origin a media element names is added to media-src: {policy}");
  std::fs::remove_dir_all(&root).unwrap();
}
