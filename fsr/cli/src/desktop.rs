//! `--desktop`: the application in a native window over the system webview
//! (WebKit on macOS and Linux, WebView2 on Windows), loading the host over
//! loopback.

use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::header::{COOKIE, HOST, SET_COOKIE};
use http::HeaderValue;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use snapfire_fsr_host::Host;

use parking_lot::Mutex;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use snapfire_fsr_host::config::Config;

use crate::BuildError;

/// What `fsr serve --desktop` binds when no `--listen` is given.
pub const LOOPBACK: &str = "127.0.0.1:0";

/// The page a scheme window opens. Windows serves it as `http://app.localhost/`.
pub const SCHEME_URL: &str = "app://localhost/";

const WAITING: &str = "<!doctype html><meta charset=utf-8><body style=\"font:13px ui-monospace,monospace;padding:16px;color:#888\">waiting for the server</body>";

const PROBE: Duration = Duration::from_millis(100);

/// The URL a window opens for a host listening on `listen`. A wildcard
/// address opens on loopback; port 0 is refused, since the port is only
/// known once something binds it.
pub fn url(listen: &str, tls: bool) -> Result<String, BuildError> {
  let addr = listen.to_socket_addrs().ok().and_then(|mut addrs| addrs.next()).ok_or_else(|| BuildError::Serve(format!("desktop: {listen} is not an address a window can open")))?;
  if addr.port() == 0 {
    return Err(BuildError::Serve(format!("desktop: {listen} names port 0; give the server a fixed port to open it in a window")));
  }
  let host = match addr.ip() {
    IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1",
    IpAddr::V6(ip) if ip.is_unspecified() => "[::1]",
    _ => listen.rsplit_once(':').map_or(listen, |(host, _)| host),
  };
  let scheme = if tls { "https" } else { "http" };
  Ok(format!("{scheme}://{host}:{}/", addr.port()))
}

/// The application's name: `[desktop] name`, else the project directory's.
pub fn name(app: &Path) -> String {
  let root = crate::serve::project_root(app);
  let configured = Config::load(&root).ok().and_then(|config| config.desktop).and_then(|desktop| desktop.name).filter(|name| !name.trim().is_empty());
  configured.unwrap_or_else(|| root.file_name().map_or_else(|| "fsr".to_owned(), |name| name.to_string_lossy().into_owned()))
}

/// The window's title over a page titled `page`: the name alone when the
/// page has no title of its own.
pub fn window_title(name: &str, page: &str) -> String {
  let page = page.trim();
  match page.is_empty() || page == name {
    true => name.to_owned(),
    false => format!("{name} - {page}"),
  }
}

enum Shown {
  Open(String),
  Title(String),
  Menu(muda::MenuId),
  Done,
}

type Stop = Box<dyn FnOnce() + Send>;

/// The window as the work beside it sees it.
pub struct Window {
  proxy: EventLoopProxy<Shown>,
  closing: Arc<Mutex<Option<Stop>>>,
}

impl Window {
  /// Loads `url` once its address accepts a connection.
  pub fn open(&self, url: String) {
    let proxy = self.proxy.clone();
    let authority = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest).split('/').next().unwrap_or_default().to_owned();
    std::thread::spawn(move || {
      while TcpStream::connect(&authority).is_err() {
        std::thread::sleep(PROBE);
      }
      let _ = proxy.send_event(Shown::Open(url));
    });
  }

  /// What closing the window runs before `run` waits for the work to end.
  /// Without one, closing the window returns at once and the work ends with
  /// the process.
  pub fn on_close(&self, stop: impl FnOnce() + Send + 'static) {
    *self.closing.lock() = Some(Box::new(stop));
  }
}

/// Answers one request to `app://`, handing the response to the callback.
pub type Handler = Arc<dyn Fn(http::Request<Vec<u8>>, Box<dyn FnOnce(http::Response<Vec<u8>>) + Send>) + Send + Sync>;

/// Runs `work` on a thread of its own and a window on this one, which must
/// be the main thread on macOS. With `scheme` the window opens `SCHEME_URL`
/// answered by it; without, it waits for `Window::open`. Returns when the
/// window is closed or when the work ends, with the work's result.
pub fn run<W>(name: &str, scheme: Option<Handler>, work: W) -> Result<(), BuildError>
where
  W: FnOnce(Window) -> Result<(), BuildError> + Send + 'static,
{
  let mut event_loop = EventLoopBuilder::<Shown>::with_user_event().build();
  let proxy = event_loop.create_proxy();
  let closing: Arc<Mutex<Option<Stop>>> = Arc::new(Mutex::new(None));
  let window = Window { proxy: proxy.clone(), closing: closing.clone() };
  let worker = std::thread::spawn(move || {
    let result = work(window);
    let _ = proxy.send_event(Shown::Done);
    result
  });

  let shown = WindowBuilder::new().with_title(name).with_inner_size(LogicalSize::new(1280.0, 800.0)).build(&event_loop).map_err(|e| BuildError::Serve(format!("desktop: window: {e}")))?;
  let titles = event_loop.create_proxy();
  let builder = WebViewBuilder::new().with_back_forward_navigation_gestures(true).with_document_title_changed_handler(move |page| {
    let _ = titles.send_event(Shown::Title(page));
  });
  let builder = match scheme {
    Some(handler) => builder
      .with_asynchronous_custom_protocol("app".to_owned(), move |_, request, responder| {
        handler(request, Box::new(move |response| responder.respond(response)));
      })
      .with_url(SCHEME_URL),
    None => builder.with_html(WAITING),
  };
  #[cfg(not(target_os = "linux"))]
  let webview = builder.build(&shown);
  #[cfg(target_os = "linux")]
  let webview = {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = shown.default_vbox().ok_or_else(|| BuildError::Serve("desktop: the window has no box to hold the webview".to_owned()))?;
    builder.build_gtk(vbox)
  };
  let webview = webview.map_err(|e| BuildError::Serve(format!("desktop: webview: {e}")))?;
  let menus = Menus::install(name, event_loop.create_proxy(), &shown)?;

  let mut closed = false;
  event_loop.run_return(|event, _, flow| {
    *flow = ControlFlow::Wait;
    match event {
      Event::UserEvent(Shown::Open(url)) => {
        if let Err(e) = webview.load_url(&url) {
          eprintln!("desktop: loading {url}: {e}");
        }
      }
      Event::UserEvent(Shown::Title(page)) => shown.set_title(&window_title(name, &page)),
      Event::UserEvent(Shown::Menu(id)) => match menus.command(&id).map(effect) {
        Some(Effect::Script(script)) => {
          let _ = webview.evaluate_script(script);
        }
        Some(Effect::Reload) => {
          let _ = webview.reload();
        }
        Some(Effect::Close) => {
          closed = true;
          *flow = ControlFlow::Exit;
        }
        None => {}
      },
      Event::UserEvent(Shown::Done) => *flow = ControlFlow::Exit,
      Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
        closed = true;
        *flow = ControlFlow::Exit;
      }
      _ => {}
    }
  });
  drop(webview);
  drop(shown);

  if closed {
    match closing.lock().take() {
      Some(stop) => stop(),
      None => return Ok(()),
    }
  }
  worker.join().unwrap_or_else(|_| Err(BuildError::Serve("desktop: the server beside the window panicked".to_owned())))
}

/// The cookies a scheme window holds for the host, since the webview keeps
/// none set by a custom scheme's responses. Reads `Max-Age` alone, which is
/// what the host writes; a cookie without one lasts the run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CookieJar {
  cookies: Vec<Held>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Held {
  name: String,
  value: String,
  expires: Option<u64>,
}

impl CookieJar {
  /// Takes one `Set-Cookie` value at `now`, seconds since the epoch.
  /// Returns whether the jar changed.
  pub fn store(&mut self, set_cookie: &str, now: u64) -> bool {
    let mut parts = set_cookie.split(';');
    let Some((name, value)) = parts.next().and_then(|pair| pair.split_once('=')) else { return false };
    let (name, value) = (name.trim(), value.trim());
    if name.is_empty() {
      return false;
    }
    let max_age = parts.filter_map(|attribute| attribute.split_once('=')).find(|(key, _)| key.trim().eq_ignore_ascii_case("max-age")).and_then(|(_, age)| age.trim().parse::<i64>().ok());
    let held = self.cookies.iter().position(|cookie| cookie.name == name);
    if max_age.is_some_and(|age| age <= 0) {
      return held.map(|at| self.cookies.remove(at)).is_some();
    }
    let cookie = Held { name: name.to_owned(), value: value.to_owned(), expires: max_age.map(|age| now + age as u64) };
    match held {
      Some(at) => self.cookies[at] = cookie,
      None => self.cookies.push(cookie),
    }
    true
  }

  /// The `Cookie` header for a request at `now`, absent when nothing is held.
  pub fn header(&self, now: u64) -> Option<String> {
    let live: Vec<String> = self.cookies.iter().filter(|cookie| cookie.expires.is_none_or(|end| end > now)).map(|cookie| format!("{}={}", cookie.name, cookie.value)).collect();
    (!live.is_empty()).then(|| live.join("; "))
  }

  /// The jar `file` holds at `now`, empty when there is no readable file.
  pub fn load(file: &Path, now: u64) -> Self {
    let mut jar: CookieJar = std::fs::read(file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    jar.cookies.retain(|cookie| cookie.expires.is_some_and(|end| end > now));
    jar
  }

  /// Writes every cookie with an end to `file`, readable by its owner alone.
  pub fn save(&self, file: &Path) -> std::io::Result<()> {
    let kept = CookieJar { cookies: self.cookies.iter().filter(|cookie| cookie.expires.is_some()).cloned().collect() };
    if let Some(dir) = file.parent() {
      std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(file)?, &serde_json::to_vec(&kept).map_err(std::io::Error::other)?)
  }
}

/// A webview's request to `app://` as the host reads one off a socket: the
/// path and query alone, `Host: localhost`, and the jar's cookies.
pub fn to_host(request: http::Request<Vec<u8>>, cookie: Option<String>) -> http::Request<Bytes> {
  let (mut parts, body) = request.into_parts();
  let path = parts.uri.path_and_query().map_or("/", |path| path.as_str()).to_owned();
  parts.uri = path.parse().unwrap_or_else(|_| http::Uri::from_static("/"));
  parts.headers.insert(HOST, HeaderValue::from_static("localhost"));
  if let Some(value) = cookie.and_then(|cookie| HeaderValue::from_str(&cookie).ok()) {
    parts.headers.insert(COOKIE, value);
  }
  http::Request::from_parts(parts, Bytes::from(body))
}

/// A response as a scheme window can take it. WebKit does not follow a
/// redirect from a custom scheme: it sends the request again, which repeats
/// an action. A redirect answering a navigation becomes a page that replaces
/// itself with the location, so the history keeps no entry to resubmit.
pub fn followable(response: http::Response<Vec<u8>>, navigation: bool) -> http::Response<Vec<u8>> {
  let location = response.headers().get(http::header::LOCATION).and_then(|value| value.to_str().ok()).map(str::to_owned);
  let (Some(location), true, true) = (location, navigation, response.status().is_redirection()) else { return response };
  let (mut parts, _) = response.into_parts();
  let attribute = location.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
  let script = serde_json::to_string(&location).unwrap_or_else(|_| "\"/\"".to_owned()).replace('<', "\\u003c");
  let page = format!("<!doctype html><meta charset=utf-8><meta http-equiv=\"refresh\" content=\"0;url={attribute}\"><script>location.replace({script})</script>");
  parts.status = http::StatusCode::OK;
  parts.headers.remove(http::header::LOCATION);
  parts.headers.remove(http::header::CONTENT_LENGTH);
  parts.headers.insert(http::header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
  http::Response::from_parts(parts, page.into_bytes())
}

/// Whether a response streams, which a scheme window cannot take: its body
/// never ends, so collecting it would hold the request for good.
pub fn streams(headers: &http::HeaderMap) -> bool {
  headers.get(http::header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).is_some_and(|kind| kind.starts_with("text/event-stream"))
}

/// The directory an application named `name` keeps its own files in.
pub fn data_dir(platform: Platform, name: &str, home: &Path, appdata: Option<&Path>, xdg_data: Option<&Path>) -> PathBuf {
  match platform {
    Platform::MacOs => home.join("Library/Application Support").join(name),
    Platform::Windows => appdata.map_or_else(|| home.join("AppData/Roaming"), Path::to_path_buf).join(name),
    Platform::Linux => xdg_data.map_or_else(|| home.join(".local/share"), Path::to_path_buf).join(name),
  }
}

/// Where `[desktop] cookies = "disk"` keeps the jar on this machine.
pub fn cookie_file(name: &str) -> Option<PathBuf> {
  let home = PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?);
  let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
  let xdg = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
  Some(data_dir(Platform::CURRENT, name, &home, appdata.as_deref(), xdg.as_deref()).join("cookies.json"))
}

/// What `[desktop] serve = "scheme"` takes away from this application.
pub fn scheme_limits(manifest: &snapfire_fsr_plan::Manifest) -> Vec<String> {
  let mut limits: Vec<String> = crate::doctor::streaming_routes(manifest).iter().map(|pattern| format!("`{pattern}` has a loading boundary; it arrives whole once its loader finishes")).collect();
  limits.push("live topics, sockets and development refresh do not connect, since a scheme response cannot stream".to_owned());
  limits
}

/// The handler a scheme window serves `host` through, run on `runtime`, with
/// the jar held in memory or in `cookies`. `log` prints a line per request.
pub fn scheme(host: Arc<Host>, runtime: tokio::runtime::Handle, cookies: Option<PathBuf>, log: bool) -> Handler {
  let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
  let jar = Arc::new(Mutex::new(cookies.as_deref().map(|file| CookieJar::load(file, now())).unwrap_or_default()));
  Arc::new(move |request, respond| {
    let (host, jar, cookies) = (host.clone(), jar.clone(), cookies.clone());
    runtime.spawn(async move {
      let cookie = jar.lock().header(now());
      let navigation = request.headers().get(http::header::ACCEPT).and_then(|value| value.to_str().ok()).is_some_and(|accept| accept.starts_with("text/html"));
      let line = log.then(|| format!("app: {} {} cookie={}", request.method(), request.uri().path_and_query().map_or("/", |p| p.as_str()), cookie.as_deref().unwrap_or("-")));
      let (parts, body) = host.handle(to_host(request, cookie)).await.into_parts();
      if let Some(line) = line {
        let set: Vec<&str> = parts.headers.get_all(SET_COOKIE).iter().filter_map(|value| value.to_str().ok()).map(|value| value.split(';').next().unwrap_or(value)).collect();
        let location = parts.headers.get(http::header::LOCATION).and_then(|value| value.to_str().ok()).map(|value| format!(" location={value}")).unwrap_or_default();
        eprintln!("{line} -> {}{location} set={}", parts.status.as_u16(), if set.is_empty() { "-".to_owned() } else { set.join(",") });
      }
      let mut changed = false;
      {
        let mut held = jar.lock();
        for value in parts.headers.get_all(SET_COOKIE).iter().filter_map(|value| value.to_str().ok()) {
          changed |= held.store(value, now());
        }
        if let (true, Some(file)) = (changed, cookies.as_deref()) {
          if let Err(e) = held.save(file) {
            eprintln!("desktop: cookies: {}: {e}", file.display());
          }
        }
      }
      if streams(&parts.headers) {
        respond(http::Response::builder().status(http::StatusCode::NO_CONTENT).body(Vec::new()).unwrap_or_default());
        return;
      }
      let body = match body.collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(e) => {
          eprintln!("desktop: {e}");
          Vec::new()
        }
      };
      respond(followable(http::Response::from_parts(parts, body), navigation));
    });
  })
}

/// A menu item the window answers itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
  Back,
  Forward,
  Reload,
  Quit,
}

impl Command {
  pub const ALL: [Command; 4] = [Command::Back, Command::Forward, Command::Reload, Command::Quit];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
  MacOs,
  Windows,
  Linux,
}

impl Platform {
  pub const CURRENT: Platform = if cfg!(target_os = "macos") {
    Platform::MacOs
  } else if cfg!(target_os = "windows") {
    Platform::Windows
  } else {
    Platform::Linux
  };
}

/// What answering a command does to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
  Script(&'static str),
  Reload,
  Close,
}

/// Quit closes the window as its close button does, so the work's stop runs.
pub fn effect(command: Command) -> Effect {
  match command {
    Command::Back => Effect::Script("history.back()"),
    Command::Forward => Effect::Script("history.forward()"),
    Command::Reload => Effect::Reload,
    Command::Quit => Effect::Close,
  }
}

/// The shortcut `command` carries on `platform`, in muda's accelerator
/// syntax. Windows' Quit has none, since Alt+F4 is the window manager's.
pub fn shortcut(command: Command, platform: Platform) -> Option<&'static str> {
  match (platform, command) {
    (Platform::MacOs, Command::Back) => Some("Cmd+["),
    (Platform::MacOs, Command::Forward) => Some("Cmd+]"),
    (Platform::MacOs, Command::Reload) => Some("Cmd+R"),
    (Platform::MacOs, Command::Quit) => Some("Cmd+Q"),
    (_, Command::Back) => Some("Alt+ArrowLeft"),
    (_, Command::Forward) => Some("Alt+ArrowRight"),
    (_, Command::Reload) => Some("Ctrl+R"),
    (Platform::Windows, Command::Quit) => None,
    (Platform::Linux, Command::Quit) => Some("Ctrl+Q"),
  }
}

fn label(command: Command, platform: Platform, name: &str) -> String {
  match (command, platform) {
    (Command::Back, _) => "Back".to_owned(),
    (Command::Forward, _) => "Forward".to_owned(),
    (Command::Reload, _) => "Reload".to_owned(),
    (Command::Quit, Platform::MacOs) => format!("Quit {name}"),
    (Command::Quit, Platform::Windows) => "Exit".to_owned(),
    (Command::Quit, Platform::Linux) => "Quit".to_owned(),
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSpec {
  pub title: String,
  pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
  Command(Command),
  Undo,
  Redo,
  Cut,
  Copy,
  Paste,
  SelectAll,
  Separator,
}

/// The menu bar on `platform`: the application's own menu first on macOS,
/// File first elsewhere, then Edit and History.
pub fn menus(platform: Platform, name: &str) -> Vec<MenuSpec> {
  let first = match platform {
    Platform::MacOs => name.to_owned(),
    Platform::Windows | Platform::Linux => "File".to_owned(),
  };
  vec![
    MenuSpec { title: first, entries: vec![Entry::Command(Command::Quit)] },
    MenuSpec { title: "Edit".to_owned(), entries: vec![Entry::Undo, Entry::Redo, Entry::Separator, Entry::Cut, Entry::Copy, Entry::Paste, Entry::SelectAll] },
    MenuSpec { title: "History".to_owned(), entries: vec![Entry::Command(Command::Back), Entry::Command(Command::Forward), Entry::Separator, Entry::Command(Command::Reload)] },
  ]
}

struct Menus {
  _bar: muda::Menu,
  commands: Vec<(muda::MenuId, Command)>,
}

impl Menus {
  fn install(name: &str, proxy: EventLoopProxy<Shown>, window: &tao::window::Window) -> Result<Self, BuildError> {
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    let failed = |e: muda::Error| BuildError::Serve(format!("desktop: menu: {e}"));
    let platform = Platform::CURRENT;
    let bar = Menu::new();
    let mut commands = Vec::new();
    for spec in menus(platform, name) {
      let submenu = Submenu::new(&spec.title, true);
      for entry in spec.entries {
        let added = match entry {
          Entry::Command(command) => {
            let item = MenuItem::new(label(command, platform, name), true, shortcut(command, platform).and_then(|keys| keys.parse().ok()));
            commands.push((item.id().clone(), command));
            submenu.append(&item)
          }
          Entry::Undo => submenu.append(&PredefinedMenuItem::undo(None)),
          Entry::Redo => submenu.append(&PredefinedMenuItem::redo(None)),
          Entry::Cut => submenu.append(&PredefinedMenuItem::cut(None)),
          Entry::Copy => submenu.append(&PredefinedMenuItem::copy(None)),
          Entry::Paste => submenu.append(&PredefinedMenuItem::paste(None)),
          Entry::SelectAll => submenu.append(&PredefinedMenuItem::select_all(None)),
          Entry::Separator => submenu.append(&PredefinedMenuItem::separator()),
        };
        added.map_err(failed)?;
      }
      bar.append(&submenu).map_err(failed)?;
    }
    attach(&bar, window)?;
    let proxy = Mutex::new(proxy);
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
      let _ = proxy.lock().send_event(Shown::Menu(event.id));
    }));
    Ok(Self { _bar: bar, commands })
  }

  fn command(&self, id: &muda::MenuId) -> Option<Command> {
    self.commands.iter().find(|(held, _)| held == id).map(|(_, command)| *command)
  }
}

#[cfg(target_os = "macos")]
fn attach(bar: &muda::Menu, _window: &tao::window::Window) -> Result<(), BuildError> {
  bar.init_for_nsapp();
  Ok(())
}

/// Not compiled on this project's machines yet.
#[cfg(target_os = "windows")]
fn attach(bar: &muda::Menu, window: &tao::window::Window) -> Result<(), BuildError> {
  use tao::platform::windows::WindowExtWindows;
  unsafe { bar.init_for_hwnd(window.hwnd() as _) }.map_err(|e| BuildError::Serve(format!("desktop: menu: {e}")))
}

/// Not compiled on this project's machines yet.
#[cfg(target_os = "linux")]
fn attach(bar: &muda::Menu, window: &tao::window::Window) -> Result<(), BuildError> {
  use tao::platform::unix::WindowExtUnix;
  bar.init_for_gtk_window(window.gtk_window(), window.default_vbox()).map_err(|e| BuildError::Serve(format!("desktop: menu: {e}")))
}
