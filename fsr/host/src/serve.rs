//! The listener: hyper over TCP or TLS and the `tower::Service` other servers nest.

use super::*;

/// What the handshake offers when `server.tls.alpn` says nothing: HTTP/2
/// first when the host negotiates it, HTTP/1.1 alone when it does not.
pub(crate) fn default_alpn(http2: bool) -> Vec<String> {
  match http2 {
    true => vec!["h2".to_owned(), "http/1.1".to_owned()],
    false => vec!["http/1.1".to_owned()],
  }
}

/// One connection, whatever it is carried over: the same edge, with HTTP/2
/// negotiated beside HTTP/1.1 when `server.http2` is on. The error is the
/// connection's, already formatted or `None` when it closed cleanly. Once `stop` turns true the connection finishes what it is answering and closes.
pub(crate) async fn serve_io<I>(host: Arc<Host>, io: I, stop: tokio::sync::watch::Receiver<bool>) -> Option<String>
where
  I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
  let http2 = host.http2;
  let service = hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
    let host = host.clone();
    async move {
      let (parts, body) = req.into_parts();
      let bytes = match http_body_util::Limited::new(body, host.max_body).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) if e.is::<http_body_util::LengthLimitError>() => return Ok::<_, Infallible>(host.too_large()),
        Err(_) => Bytes::new(),
      };
      let version = parts.version;
      let mut response = host.handle(Request::from_parts(parts, bytes)).await;
      // HTTP/1.0 has no chunked encoding, so a body of unknown length ends with the connection; hyper keeps it open for a 1.1 response.
      if version == http::Version::HTTP_10 && !response.headers().contains_key(header::CONTENT_LENGTH) && http_body::Body::size_hint(response.body()).exact().is_none() {
        *response.version_mut() = http::Version::HTTP_10;
      }
      Ok::<_, Infallible>(response)
    }
  });
  if http2 {
    let builder = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
    let connection = builder.serve_connection_with_upgrades(io, service);
    until_stopped(connection, stop, |c| c.graceful_shutdown()).await.err().map(|e| e.to_string())
  } else {
    let connection = hyper::server::conn::http1::Builder::new().serve_connection(io, service).with_upgrades();
    until_stopped(connection, stop, |c| c.graceful_shutdown()).await.err().map(|e| e.to_string())
  }
}

/// Runs `connection` to its end, asking it to close once `stop` turns true. `stop` is held until then, which is how the listener counts what it still waits for.
async fn until_stopped<C, T, E>(connection: C, mut stop: tokio::sync::watch::Receiver<bool>, close: impl FnOnce(Pin<&mut C>)) -> Result<T, E>
where
  C: Future<Output = Result<T, E>>,
{
  let mut connection = std::pin::pin!(connection);
  tokio::select! {
    done = connection.as_mut() => return done,
    _ = stop.wait_for(|stopped| *stopped) => {}
  }
  close(connection.as_mut());
  let done = connection.await;
  drop(stop);
  done
}

/// Resolves on SIGTERM or SIGINT. Where there are no signals it resolves on Ctrl-C.
pub async fn shutdown_signal() {
  #[cfg(unix)]
  {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
      Ok(mut term) => {
        tokio::select! {
          _ = term.recv() => {}
          _ = tokio::signal::ctrl_c() => {}
        }
        return;
      }
      Err(e) => tracing::warn!(target: "fsr::host", error = %e, "cannot listen for SIGTERM; only SIGINT stops the server"),
    }
  }
  let _ = tokio::signal::ctrl_c().await;
}

/// `tower::Service` over the host, for hyper, axum or any tower stack.
#[derive(Clone)]
pub struct HostService(pub Arc<Host>);

impl<B> tower::Service<Request<B>> for HostService
where
  B: http_body::Body + Send + 'static,
  B::Data: Send,
  B::Error: std::fmt::Debug,
{
  type Response = Response<Body>;
  type Error = Infallible;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request<B>) -> Self::Future {
    let host = self.0.clone();
    Box::pin(async move {
      let (parts, body) = req.into_parts();
      let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
      };
      Ok(host.handle(Request::from_parts(parts, bytes)).await)
    })
  }
}

impl Host {
  /// Serves on `listen` with hyper. `Host::listen` is the configured address.
  pub async fn serve(self: Arc<Self>, listen: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    self.serve_listener(listener).await
  }

  /// Serves an already bound listener, which is how a test picks port zero, until [`shutdown_signal`].
  pub async fn serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    self.serve_until(listener, shutdown_signal()).await
  }

  /// Turns `server.ready` to 503 `shutting down` for good. `serve_until` calls it when its shutdown resolves; a server that owns the listener, an actix `HttpServer` or an axum router the host is nested in, calls it before it stops, so a balancer probing readiness stops routing here while the open requests finish.
  pub fn begin_drain(&self) {
    self.draining.store(true, std::sync::atomic::Ordering::Relaxed);
  }

  /// Whether `begin_drain` has been called.
  pub fn draining(&self) -> bool {
    self.draining.load(std::sync::atomic::Ordering::Relaxed)
  }

  /// Serves `listener` until `shutdown` resolves, then stops accepting and waits up to `server.shutdown_grace` for the connections it holds to finish what they are answering and for the work their requests left with `after`.
  pub async fn serve_until(self: Arc<Self>, listener: tokio::net::TcpListener, shutdown: impl Future<Output = ()>) -> std::io::Result<()> {
    #[cfg(all(unix, feature = "tls"))]
    self.clone().watch_for_reload();
    let (stop, held) = tokio::sync::watch::channel(false);
    let mut shutdown = std::pin::pin!(shutdown);
    loop {
      tokio::select! {
        accepted = listener.accept() => {
          let (stream, _) = accepted?;
          let host = self.clone();
          let held = held.clone();
          tokio::spawn(async move { host.serve_connection(stream, held).await });
        }
        () = shutdown.as_mut() => break,
      }
    }
    self.begin_drain();
    drop(listener);
    drop(held);
    let _ = stop.send(true);
    let open = stop.receiver_count();
    tracing::info!(target: "fsr::host", connections = open, grace_s = self.shutdown_grace.as_secs(), "shutting down");
    let deadline = tokio::time::Instant::now() + self.shutdown_grace;
    if tokio::time::timeout_at(deadline, stop.closed()).await.is_err() {
      tracing::warn!(target: "fsr::host", connections = stop.receiver_count(), "server.shutdown_grace ran out with connections still open; they are cut");
    }
    if tokio::time::timeout_at(deadline, self.after_settled()).await.is_err() {
      tracing::warn!(target: "fsr::host", "server.shutdown_grace ran out with work after a response still running; it is cut");
    }
    Ok(())
  }

  pub(crate) async fn serve_connection(self: Arc<Self>, stream: tokio::net::TcpStream, stop: tokio::sync::watch::Receiver<bool>) {
    #[cfg(feature = "tls")]
    if let Some(tls) = self.tls.clone() {
      let stream = match tls.acceptor().accept(stream).await {
        Ok(stream) => stream,
        Err(e) => {
          tracing::debug!(target: "fsr::host", error = %e, "handshake failed");
          return;
        }
      };
      if let Some(e) = serve_io(self, hyper_util::rt::TokioIo::new(stream), stop).await {
        tracing::debug!(target: "fsr::host", error = %e, "connection ended");
      }
      return;
    }
    if let Some(e) = serve_io(self, hyper_util::rt::TokioIo::new(stream), stop).await {
      tracing::debug!(target: "fsr::host", error = %e, "connection ended");
    }
  }

  /// Re-reads the certificate and its key and swaps what the next handshake
  /// presents; connections already up are untouched and a file that will not
  /// read leaves the running certificate in place. Nothing without
  /// `[server.tls]`. The configured signal calls this and so may a caller.
  #[cfg(feature = "tls")]
  pub fn reload_tls(&self) -> Result<(), HostError> {
    match &self.tls {
      Some(tls) => tls.reload(),
      None => Ok(()),
    }
  }

  #[cfg(all(unix, feature = "tls"))]
  pub(crate) fn watch_for_reload(self: Arc<Self>) {
    use tokio::signal::unix::{SignalKind, signal};
    let Some(name) = self.tls_reload.clone() else { return };
    let kind = match name.as_str() {
      "hup" => SignalKind::hangup(),
      "usr1" => SignalKind::user_defined1(),
      "usr2" => SignalKind::user_defined2(),
      _ => return,
    };
    tokio::spawn(async move {
      let mut stream = match signal(kind) {
        Ok(stream) => stream,
        Err(e) => {
          tracing::warn!(target: "fsr::host", error = %e, "cannot listen for SIG{}", name.to_uppercase());
          return;
        }
      };
      while stream.recv().await.is_some() {
        match self.reload_tls() {
          Ok(()) => tracing::info!(target: "fsr::host", "certificate reloaded"),
          Err(e) => tracing::warn!(target: "fsr::host", error = %e, "certificate not reloaded; the running one stands"),
        }
      }
    });
  }

  pub fn listen(&self) -> &str {
    &self.report_listen
  }

  pub(crate) fn too_large(&self) -> Response<Body> {
    text_response(
      StatusCode::PAYLOAD_TOO_LARGE,
      format!("request body over {} bytes, the host's server.max_body", self.max_body),
    )
  }

  pub fn service(self: &Arc<Self>) -> HostService {
    HostService(self.clone())
  }
}
