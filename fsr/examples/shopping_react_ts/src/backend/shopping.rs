//! The second server: a service this example does not own, reached only over
//! HTTP and described only by the document it publishes.

use actix_web::web::{self, Data, Json, Path, Query};
use actix_web::{App, HttpResponse, HttpServer};
use serde::Deserialize;

use super::catalog::{Catalog, Filter, OrderRequest};

/// The only description of this API anything else is allowed to read. It is
/// included from `app/clients/` rather than copied, so the document the build
/// imports and the document this server publishes cannot disagree.
pub const DOCUMENT: &str = include_str!("../../app/clients/shopping.openapi.json");

#[derive(Deserialize)]
struct ListQuery {
  q: Option<String>,
  category: Option<String>,
  tag: Option<String>,
  /// The example's failure switch: the catalogue answers 503 so one segment
  /// can degrade while the rest of the page renders.
  fail: Option<u8>,
}

async fn list_products(catalog: Data<Catalog>, query: Query<ListQuery>) -> HttpResponse {
  if query.fail.is_some_and(|f| f != 0) {
    return HttpResponse::ServiceUnavailable().body("catalog is unreachable");
  }
  let filter = Filter { q: query.q.as_deref(), category: query.category.as_deref(), tag: query.tag.as_deref() };
  HttpResponse::Ok().json(catalog.list(&filter))
}

async fn get_product(catalog: Data<Catalog>, id: Path<u64>) -> HttpResponse {
  match catalog.get(*id) {
    Some(product) => HttpResponse::Ok().json(product),
    None => HttpResponse::NotFound().body(format!("no product {}", *id)),
  }
}

async fn place_order(catalog: Data<Catalog>, body: Json<OrderRequest>) -> HttpResponse {
  match catalog.place(&body) {
    Ok(order) => HttpResponse::Created().json(order),
    Err(e) => HttpResponse::build(actix_web::http::StatusCode::from_u16(e.status()).unwrap())
      .body(e.message()),
  }
}

async fn get_order(catalog: Data<Catalog>, id: Path<u64>) -> HttpResponse {
  match catalog.order(*id) {
    Some(order) => HttpResponse::Ok().json(order),
    None => HttpResponse::NotFound().body(format!("no order {}", *id)),
  }
}

async fn document() -> HttpResponse {
  HttpResponse::Ok()
    .content_type("application/json; charset=utf-8")
    .body(DOCUMENT)
}

pub async fn serve(catalog: Catalog, addr: (&str, u16)) -> std::io::Result<()> {
  HttpServer::new(move || {
    App::new()
      .app_data(Data::new(catalog.clone()))
      .route("/openapi.json", web::get().to(document))
      .route("/products", web::get().to(list_products))
      .route("/products/{id}", web::get().to(get_product))
      .route("/orders", web::post().to(place_order))
      .route("/orders/{id}", web::get().to(get_order))
  })
  .bind(addr)?
  .run()
  .await
}
