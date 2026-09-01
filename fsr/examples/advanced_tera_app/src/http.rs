use std::sync::Arc;

use actix_web::http::header;
use actix_web::web::{Bytes, Data};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::StreamExt;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::RequestCtx;
use snapfire_fsr_session::Opened;

use crate::render::{call_action, negotiate_encoding, respond_with, AppError, RenderMode};
use crate::AppCore;

fn query_param<'q>(query: &'q str, key: &str) -> Option<&'q str> {
  query.split('&').find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
}

async fn handle_action(req: &HttpRequest, app: &AppCore, opened: &Opened, body: Bytes) -> HttpResponse {
  let id = req.path().trim_start_matches("/_sf/action/");
  let is_form = req
    .headers()
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));

  let input = if is_form {
    let mut map = ValueMap::new();
    for (k, v) in form_urlencoded::parse(&body) {
      map.insert(k.into_owned(), Value::Str(v.into_owned()));
    }
    let token = match map.shift_remove("_csrf") {
      Some(Value::Str(token)) => token,
      _ => String::new(),
    };
    if !app.sessions.verify_csrf(&opened.id, &token) {
      return HttpResponse::Forbidden().body("csrf verification failed");
    }
    Value::Map(map)
  } else {
    match serde_json::from_slice(&body)
      .map_err(|e| e.to_string())
      .and_then(|j| json_to_value(&j).map_err(|e| e.to_string()))
    {
      Ok(v) => v,
      Err(e) => return HttpResponse::BadRequest().body(format!("invalid action input: {e}")),
    }
  };

  let ctx = RequestCtx {
    params: Default::default(),
    session: opened.cell.clone(),
    csrf: None,
  };

  match call_action(app, id, ctx, input).await {
    Ok(_) if is_form => {
      let back = req
        .headers()
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("/");
      HttpResponse::SeeOther().insert_header((header::LOCATION, back)).finish()
    }
    Ok(result) => HttpResponse::Ok().json(value_to_json(&result)),
    Err(e) => HttpResponse::build(
      actix_web::http::StatusCode::from_u16(e.kind.http_status()).unwrap(),
    )
    .json(serde_json::json!({ "kind": e.kind.as_str(), "message": e.message })),
  }
}

async fn handle(req: HttpRequest, app: Data<Arc<AppCore>>, body: Bytes) -> HttpResponse {
  let cookie_header = req
    .headers()
    .get(header::COOKIE)
    .and_then(|v| v.to_str().ok())
    .map(str::to_owned);
  let opened = app.sessions.open(cookie_header.as_deref()).await;

  let mut response = route(&req, &app, &opened, body).await;
  if let Some(set_cookie) = app.sessions.persist(&opened).await {
    if let Ok(value) = header::HeaderValue::from_str(&set_cookie) {
      response.headers_mut().append(header::SET_COOKIE, value);
    }
  }
  response
}

async fn route(req: &HttpRequest, app: &AppCore, opened: &Opened, body: Bytes) -> HttpResponse {
  if req.path().starts_with("/_sf/action/") && req.method() == actix_web::http::Method::POST {
    return handle_action(req, app, opened, body).await;
  }

  let query = req.query_string();
  let mode = if query_param(query, "__payload").is_some() || query.split('&').any(|p| p == "__payload") {
    RenderMode::Payload
  } else {
    RenderMode::Html
  };
  if mode == RenderMode::Payload {
    if let Err(e) = negotiate_encoding(query_param(query, "enc")) {
      return HttpResponse::NotAcceptable().body(e.to_string());
    }
  }

  tracing::info!(target: "fsr::http", path = req.path(), payload = (mode == RenderMode::Payload), "request");
  let csrf = app.sessions.csrf_token(&opened.id);
  match respond_with(app, req.path(), mode, opened.cell.clone(), Some(csrf)).await {
    Ok(chunks) => {
      let content_type = match mode {
        RenderMode::Html => "text/html; charset=utf-8",
        RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
      };
      HttpResponse::Ok()
        .content_type(content_type)
        .streaming(chunks.map(|c| Ok::<_, actix_web::Error>(Bytes::from(c))))
    }
    Err(AppError::NotFound(path)) => HttpResponse::NotFound().body(format!("no route: {path}")),
    Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
  }
}

pub async fn serve(app: Arc<AppCore>, addr: (&str, u16)) -> std::io::Result<()> {
  let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let client_dist = manifest.join("../../client/dist");
  let app_dist = manifest.join("js/dist");
  let vendor = manifest.join("vendor");
  HttpServer::new(move || {
    App::new()
      .app_data(Data::new(app.clone()))
      .service(actix_files::Files::new("/static/js/fsr", client_dist.clone()))
      .service(actix_files::Files::new("/static/js/app", app_dist.clone()))
      .service(actix_files::Files::new("/static/js/vendor", vendor.clone()))
      .default_service(web::to(handle))
  })
  .bind(addr)?
  .run()
  .await
}
