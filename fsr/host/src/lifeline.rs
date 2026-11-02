//! A server `fsr dev` started does not outlive it, even when `fsr dev` is killed and runs nothing on the way out.

use std::io::{self, Read};
use std::sync::Once;
use std::time::Duration;

/// Set by whatever starts a host and holds the write end of its stdin. The host stops when that end closes.
pub const LIFELINE: &str = "FSR_HOST_LIFELINE";

/// How long the application's own shutdown has before the process exits anyway.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Watches stdin when [`LIFELINE`] is set. A signal rather than an exit, so a server shuts down the way it does for anything else.
pub(crate) fn hold() {
  static WATCHING: Once = Once::new();
  if std::env::var_os(LIFELINE).is_none_or(|value| value.is_empty()) {
    return;
  }
  WATCHING.call_once(|| {
    std::thread::spawn(|| {
      if let Err(e) = io::copy(&mut io::stdin().lock().by_ref(), &mut io::sink()) {
        tracing::warn!(target: "fsr::host", error = %e, "cannot read the lifeline on stdin; this server will not stop with what started it");
        return;
      }
      terminate();
      std::thread::sleep(SHUTDOWN_GRACE);
      std::process::exit(0);
    });
  });
}

#[cfg(unix)]
fn terminate() {
  // SAFETY: kill with this process's own id and a valid signal touches no memory.
  unsafe {
    libc::kill(libc::getpid(), libc::SIGTERM);
  }
}

#[cfg(not(unix))]
fn terminate() {}
