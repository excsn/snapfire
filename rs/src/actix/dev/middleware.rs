use actix_web::{
  Error,
  body::{BoxBody, MessageBody},
  dev::{Service, ServiceRequest, ServiceResponse, Transform},
  http::header::CONTENT_TYPE,
};
use bytes::{Bytes, BytesMut};
use futures_util::future::{self, LocalBoxFuture};
use std::{rc::Rc, task::Poll};

const SCRIPT_TAG_START: &[u8] = b"<script data-snapfire-reload=\"true\">";
const SCRIPT_CONTENT: &[u8] = include_bytes!("injected.js");
const SCRIPT_TAG_END: &[u8] = b"</script>";
const BODY_TAG: &[u8] = b"</body>";

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
  // The service is now wrapped in an Rc
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
    // Clone the Rc to get an owned handle to the service.
    // This handle can be moved into the async block.
    let service = self.service.clone();

    Box::pin(async move {
      let res = service.call(req).await?;

      let is_html = res
        .headers()
        .get(CONTENT_TYPE)
        .map_or(false, |val| val.to_str().unwrap_or("").contains("text/html"));

      if !is_html {
        return Ok(res.map_into_boxed_body());
      }

      let res = res.map_body(move |_head, body| {
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

          let new_body = if let Some(body_end_index) = find_case_insensitive(&body_bytes, BODY_TAG) {
            let new_body_len = body_bytes.len() + SCRIPT_TAG_START.len() + SCRIPT_CONTENT.len() + SCRIPT_TAG_END.len();
            let mut new_body = BytesMut::with_capacity(new_body_len);

            new_body.extend_from_slice(&body_bytes[..body_end_index]);
            new_body.extend_from_slice(SCRIPT_TAG_START);
            new_body.extend_from_slice(SCRIPT_CONTENT);
            new_body.extend_from_slice(SCRIPT_TAG_END);
            new_body.extend_from_slice(&body_bytes[body_end_index..]);
            new_body.freeze()
          } else {
            // If no body tag, append it all at the end
            let new_body_len = body_bytes.len() + SCRIPT_TAG_START.len() + SCRIPT_CONTENT.len() + SCRIPT_TAG_END.len();
            let mut new_body = BytesMut::with_capacity(new_body_len);

            new_body.extend_from_slice(&body_bytes);
            new_body.extend_from_slice(SCRIPT_TAG_START);
            new_body.extend_from_slice(SCRIPT_CONTENT);
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
