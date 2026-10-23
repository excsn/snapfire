//! Work a request left to run once its response is written, `after(fn)`.

use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use http_body::{Body as HttpBody, Frame, SizeHint};
use snapfire_fsr_runtime::AfterHandle;
use tokio::sync::Notify;

tokio::task_local! {
  /// The queue of the request being handled, which every context the host builds for it shares.
  pub(crate) static AFTER: AfterHandle;
}

/// The work running for every request this host answered.
#[derive(Default)]
pub(crate) struct AfterTasks {
  running: AtomicUsize,
  idle: Notify,
}

impl AfterTasks {
  /// Spawns everything `queued` holds. Without a runtime to spawn on, the work is dropped with a warning.
  pub(crate) fn start(self: &Arc<Self>, queued: AfterHandle) {
    let work = queued.take();
    if work.is_empty() {
      return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
      tracing::warn!(target: "fsr::after", pieces = work.len(), "no runtime to run the work after the response on");
      return;
    };
    for piece in work {
      self.running.fetch_add(1, Ordering::SeqCst);
      let tasks = self.clone();
      runtime.spawn(async move {
        piece.await;
        if tasks.running.fetch_sub(1, Ordering::SeqCst) == 1 {
          tasks.idle.notify_waiters();
        }
      });
    }
  }

  /// Resolves once nothing started here is still running.
  pub(crate) async fn settled(&self) {
    loop {
      let notified = self.idle.notified();
      tokio::pin!(notified);
      notified.as_mut().enable();
      if self.running.load(Ordering::SeqCst) == 0 {
        return;
      }
      notified.await;
    }
  }
}

/// A response body that starts its request's work once it ends or is dropped, whichever comes first.
pub(crate) struct Finishing<B> {
  inner: B,
  after: Option<(AfterHandle, Arc<AfterTasks>)>,
}

impl<B> Finishing<B> {
  pub(crate) fn new(inner: B, queued: AfterHandle, tasks: Arc<AfterTasks>) -> Self {
    Self { inner, after: Some((queued, tasks)) }
  }

  fn finish(&mut self) {
    if let Some((queued, tasks)) = self.after.take() {
      tasks.start(queued);
    }
  }
}

impl<B> Drop for Finishing<B> {
  fn drop(&mut self) {
    self.finish();
  }
}

impl<B: HttpBody + Unpin> HttpBody for Finishing<B> {
  type Data = B::Data;
  type Error = B::Error;

  fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
    let polled = Pin::new(&mut self.inner).poll_frame(cx);
    if let Poll::Ready(None) = polled {
      self.finish();
    }
    polled
  }

  fn is_end_stream(&self) -> bool {
    self.inner.is_end_stream()
  }

  fn size_hint(&self) -> SizeHint {
    self.inner.size_hint()
  }
}
