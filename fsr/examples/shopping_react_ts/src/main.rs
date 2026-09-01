use shopping_react_ts::backend;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  let catalog = backend::Catalog::seed();
  println!("shopping backend on http://127.0.0.1:8081/products");
  backend::serve(catalog, ("127.0.0.1", 8081)).await
}
