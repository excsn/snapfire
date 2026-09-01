pub mod catalog;
pub mod openapi;

use actix_web::web::{Data, Json, Path, Query};
use actix_web::{App, HttpResponse, HttpServer};
use serde::Deserialize;

pub use catalog::Catalog;
use catalog::OrderRequest;

#[derive(Deserialize)]
struct ListQuery {
  tag: Option<String>,
  /// The example's failure switch: the catalogue answers 503 so one segment
  /// can degrade while the rest of the page renders.
  fail: Option<u8>,
}

async fn list_products(catalog: Data<Catalog>, query: Query<ListQuery>) -> HttpResponse {
  if query.fail.is_some_and(|f| f != 0) {
    return HttpResponse::ServiceUnavailable().body("catalog is unreachable");
  }
  HttpResponse::Ok().json(catalog.list(query.tag.as_deref()))
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

async fn document() -> HttpResponse {
  HttpResponse::Ok()
    .content_type("application/json; charset=utf-8")
    .body(openapi::DOCUMENT)
}

/// The second server: a service this example does not own, reached only over
/// HTTP and described only by the document it publishes.
pub async fn serve(catalog: Catalog, addr: (&str, u16)) -> std::io::Result<()> {
  HttpServer::new(move || {
    App::new()
      .app_data(Data::new(catalog.clone()))
      .route("/openapi.json", actix_web::web::get().to(document))
      .route("/products", actix_web::web::get().to(list_products))
      .route("/products/{id}", actix_web::web::get().to(get_product))
      .route("/orders", actix_web::web::post().to(place_order))
  })
  .bind(addr)?
  .run()
  .await
}
