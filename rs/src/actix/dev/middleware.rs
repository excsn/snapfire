use actix_web::{
  Error,
  body::{BoxBody, MessageBody},
  dev::{Service, ServiceRequest, ServiceResponse, Transform},
  http::header::CONTENT_TYPE,
};
use bytes::BytesMut;
use futures_util::future::{self, LocalBoxFuture};
use std::{rc::Rc, task::Poll};

use crate::core::app::{DEFAULT_WS_PATH, TeraWeb};
use crate::core::reload::client_script;

const SCRIPT_TAG_START: &[u8] = b"<script data-snapfire-reload=\"true\">";
const SCRIPT_TAG_END: &[u8] = b"</script>";
const BODY_TAG: &[u8] = b"</body>";

/// Actix middleware that inserts the live-reload script into HTML responses.
///
/// The script is placed immediately before the closing `</body>` tag, or appended when
/// the response has no such tag. Responses whose `Content-Type` is not `text/html` pass
/// through untouched.
///
/// The WebSocket URL written into the script is read from the [`TeraWeb`] registered as
/// Actix app data, so it follows [`TeraWebBuilder::ws_path`]. Without that app data the
/// default path is used.
///
/// [`TeraWebBuilder::ws_path`]: crate::TeraWebBuilder::ws_path
///
/// # Examples
///
/// ```no_run
/// # use actix_web::{App, web};
/// # let app_state: snapfire::TeraWeb = unimplemented!();
/// App::new()
///   .app_data(web::Data::new(app_state))
///   .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
/// # ;
/// ```
#[derive(Debug, Clone, Default)]
pub struct InjectSnapFireScript;

impl<S, B> Transform<S, ServiceRequest> for InjectSnapFireScript
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  B: MessageBody + 'static,
{
  type Response = ServiceResponse<BoxBody>;
  type Error = Error;
  type Transform = InjectSnapFireScriptMiddleware<S>;
  type InitError = ();
  type Future = future::Ready<Result<Self::Transform, Self::InitError>>;

  fn new_transform(&self, service: S) -> Self::Future {
    future::ok(InjectSnapFireScriptMiddleware {
      // Wrap the service in an Rc so it can be shared and owned by futures
      service: Rc::new(service),
    })
  }
}

pub struct InjectSnapFireScriptMiddleware<S> {
  service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for InjectSnapFireScriptMiddleware<S>
where
  S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
  B: MessageBody + 'static,
{
  type Response = ServiceResponse<BoxBody>;
  type Error = Error;
  type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

  fn poll_ready(&self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
    self.service.poll_ready(cx)
  }

  fn call(&self, req: ServiceRequest) -> Self::Future {
    let service = self.service.clone();

    let state = req.app_data::<actix_web::web::Data<TeraWeb>>();
    let script = state.is_none_or(|state| state.auto_inject_script).then(|| {
      let ws_path = state.map_or(DEFAULT_WS_PATH, |state| state.reloader.ws_path.as_str());
      client_script(ws_path)
    });

    Box::pin(async move {
      let res = service.call(req).await?;

      let is_html = res
        .headers()
        .get(CONTENT_TYPE)
        .is_some_and(|val| val.to_str().unwrap_or("").contains("text/html"));

      let Some(script) = script.filter(|_| is_html) else {
        return Ok(res.map_into_boxed_body());
      };

      let res = res.map_body(move |_head, body| {
        let script = script.into_bytes();
        let body_fut = async move {
          let body_bytes = match actix_web::body::to_bytes(body).await {
            Ok(bytes) => {
              bytes
            }
            Err(_e) => {
              return Err(actix_web::error::ErrorInternalServerError(
                "Failed to buffer response body",
              ));
            }
          };

          let new_body_len = body_bytes.len() + SCRIPT_TAG_START.len() + script.len() + SCRIPT_TAG_END.len();

          let new_body = if let Some(body_end_index) = find_case_insensitive(&body_bytes, BODY_TAG) {
            let mut new_body = BytesMut::with_capacity(new_body_len);

            new_body.extend_from_slice(&body_bytes[..body_end_index]);
            new_body.extend_from_slice(SCRIPT_TAG_START);
            new_body.extend_from_slice(&script);
            new_body.extend_from_slice(SCRIPT_TAG_END);
            new_body.extend_from_slice(&body_bytes[body_end_index..]);
            new_body.freeze()
          } else {
            let mut new_body = BytesMut::with_capacity(new_body_len);

            new_body.extend_from_slice(&body_bytes);
            new_body.extend_from_slice(SCRIPT_TAG_START);
            new_body.extend_from_slice(&script);
            new_body.extend_from_slice(SCRIPT_TAG_END);
            new_body.freeze()
          };

          Ok::<_, Error>(new_body)
        };

        actix_web::body::BodyStream::new(Box::pin(async_stream::stream! {
          yield body_fut.await;
        }))
        .boxed()
      });

      Ok(res)
    })
  }
}

fn find_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack
    .windows(needle.len())
    .position(|window| window.eq_ignore_ascii_case(needle))
}
