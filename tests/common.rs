use actix_web::{web, Responder};
use snapfire::TeraWeb;
use tera::Context;

// Test handler that uses the snapfire render method
pub async fn test_handler(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Integration Test");
  app_state.render("index.html", context)
}
