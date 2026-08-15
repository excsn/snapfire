use actix_web::{App, HttpServer, Responder, web};
use snapfire::TeraWeb;
use tera::Context;

#[derive(serde::Serialize)]
struct Post {
  slug: String,
  title: String,
  body: String,
}

fn posts() -> Vec<Post> {
  vec![
    Post {
      slug: "hello".to_string(),
      title: "Hello, world".to_string(),
      body: "The first post.".to_string(),
    },
    Post {
      slug: "tera-2".to_string(),
      title: "Moving to Tera 2".to_string(),
      body: "Macros became components.".to_string(),
    },
  ]
}

async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Posts");
  context.insert("posts", &posts());
  app_state.render("index.html", context)
}

async fn post(app_state: web::Data<TeraWeb>, slug: web::Path<String>) -> impl Responder {
  let all = posts();
  let found = all.iter().find(|p| p.slug == *slug);

  let mut context = Context::new();
  match found {
    Some(post) => {
      context.insert("page_title", &post.title);
      context.insert("post", post);
      app_state.render("post.html", context)
    }
    None => {
      context.insert("page_title", "Not found");
      context.insert("slug", &*slug);
      app_state.render("not_found.html", context)
    }
  }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  let mut templates_glob = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  templates_glob.push("examples/inheritance/templates/**/*.html");

  let app_state = TeraWeb::builder(templates_glob.to_str().unwrap())
    .add_global("site_name", "SnapFire Journal")
    .add_global("version", env!("CARGO_PKG_VERSION"))
    .build()
    .expect("Failed to build TeraWeb app");

  log::info!("🚀 http://127.0.0.1:3001");

  HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state.clone()))
      .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
      .route("/", web::get().to(index))
      .route("/posts/{slug}", web::get().to(post))
      .configure(|cfg| app_state.configure_routes(cfg))
  })
  .bind(("127.0.0.1", 3001))?
  .run()
  .await
}
