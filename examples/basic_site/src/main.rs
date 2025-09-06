use actix_files::Files;
use actix_web::{web, App, HttpServer, Responder};
use snapfire::{Template, TeraWeb};
use tera::Context;

#[derive(serde::Serialize)]
struct User {
  name: String,
  email: String,
}

/// Renders the home page.
async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Home");
  context.insert("message", "Welcome to the SnapFire demo site!");
  app_state.render("index.html", context)
}

/// Renders a page with more complex context.
async fn user_profile(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "User Profile");
  context.insert(
    "user",
    &User {
      name: "Alice".to_string(),
      email: "alice@example.com".to_string(),
    },
  );
  app_state.render("user.html", context)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
  // Initialize logging
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
  let mut templates_path = std::path::PathBuf::from(manifest_dir.clone());
  templates_path.push("templates/**/*.html");
  
  let mut static_path_to_watch = std::path::PathBuf::from(manifest_dir.clone());
  static_path_to_watch.push("static");

  let mut static_path_for_service = std::path::PathBuf::from(manifest_dir);
  static_path_for_service.push("static");

  // 1. Configure and build the SnapFire state.
  let app_state = TeraWeb::builder(templates_path.to_str().unwrap())
    .add_global("site_name", "SnapFire Demo")
    .add_global("version", env!("CARGO_PKG_VERSION"))
    // Watch the static directory for CSS changes
    .watch_static(static_path_to_watch.to_str().unwrap())
    .build()
    .expect("Failed to build TeraWeb app");

  log::info!("🚀 Starting server at http://127.0.0.1:3000");

  HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state.clone()))
      // 2. [devel only] Inject the dev middleware.
      .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
      .service(Files::new("/static", static_path_for_service.clone()))
      .route("/", web::get().to(index))
      .route("/profile", web::get().to(user_profile))
      // 3. [devel only] Configure dev routes (the WebSocket).
      .configure(|cfg| app_state.configure_routes(cfg))
  })
  .bind(("127.0.0.1", 3000))?
  .run()
  .await
}
