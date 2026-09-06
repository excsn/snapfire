use actix_web::{App, HttpServer, Responder, web};
use snapfire::TeraWeb;
use tera::{Context, Kwargs, State, TeraResult};

#[derive(serde::Serialize, serde::Deserialize)]
struct Product {
  name: String,
  cents: i64,
}

#[derive(serde::Deserialize)]
struct TotalArgs {
  of: Vec<Product>,
}

fn upcase(value: &str, _: Kwargs, _: &State) -> String {
  value.to_uppercase()
}

fn money(value: i64, kwargs: Kwargs, _: &State) -> TeraResult<String> {
  let symbol: Option<&str> = kwargs.get("symbol")?;
  Ok(format!("{}{}.{:02}", symbol.unwrap_or("$"), value / 100, value % 100))
}

fn total(kwargs: Kwargs, _: &State) -> TeraResult<i64> {
  let args: TotalArgs = kwargs.deserialize()?;
  Ok(args.of.iter().map(|p| p.cents).sum())
}

async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
  let mut context = Context::new();
  context.insert("page_title", "Catalogue");
  context.insert(
    "products",
    &[
      Product {
        name: "widget".to_string(),
        cents: 1250,
      },
      Product {
        name: "sprocket".to_string(),
        cents: 899,
      },
    ],
  );
  app_state.render("index.html", context)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  let mut templates_glob = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  templates_glob.push("examples/custom_filters/templates/**/*.html");

  let app_state = TeraWeb::builder(templates_glob.to_str().unwrap())
    .add_global("site_name", "SnapFire Shop")
    // Tera resolves filter, function, test and component names while it parses,
    // so registrations have to land on the instance before the glob is loaded.
    // SnapFire runs this closure first for that reason.
    .configure_tera(|tera| {
      tera.register_filter("upcase", upcase);
      tera.register_filter("money", money);
      tera.register_function("total", total);
    })
    .build()
    .expect("Failed to build TeraWeb app");

  log::info!("🚀 http://127.0.0.1:3000");

  HttpServer::new(move || {
    App::new()
      .app_data(web::Data::new(app_state.clone()))
      .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
      .route("/", web::get().to(index))
      .configure(|cfg| app_state.configure_routes(cfg))
  })
  .bind(("127.0.0.1", 3000))?
  .run()
  .await
}
