use actix_files::Files;
use actix_web::{App, HttpServer, Responder, web};
use snapfire::TeraWeb;
use tera::Context;

async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Live reload");
  app_state.render("index.html", context)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/live_reload");
  let templates_glob = root.join("templates/**/*.html");
  let static_dir = root.join("static");

  let app_state = TeraWeb::builder(templates_glob.to_str().unwrap())
    .add_global("site_name", "SnapFire Live")
    // Editing a .html under templates/ triggers a full page reload; editing a
    // .css under static/ swaps the stylesheet without navigating.
    .watch_static(static_dir.to_str().unwrap())
    .ws_path("/_dev/socket")
    .build()
    .expect("Failed to build TeraWeb app");

  log::info!("🚀 http://127.0.0.1:3002");
  log::info!("edit examples/live_reload/templates/index.html or static/style.css and watch the page");

  HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state.clone()))
      .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
      .service(Files::new("/static", static_dir.clone()))
      .route("/", web::get().to(index))
      .configure(|cfg| app_state.configure_routes(cfg))
  })
  .bind(("127.0.0.1", 3002))?
  .run()
  .await
}
