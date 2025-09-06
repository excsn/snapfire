//! # SnapFire
//!
//! An ergonomic Tera templating engine with live-reload, featuring first-class
//! support for Actix Web.
//!
//! ## Features
//!
//! - **Simple API:** A clean builder pattern for easy setup.
//! - **Full Context Control:** Pass a `tera::Context` directly to the render method.
//! - **Global Variables:** Define site-wide variables available to all templates.
//! - **Live Reload (Dev Mode):** Changes to templates or static files (like CSS) are
//!   automatically streamed to the browser without a full page refresh.
//! - **Production Optimized:** All development features are completely compiled out
//!   in release builds for zero overhead.
//!
//! ## Quickstart
//!
//! This example demonstrates how to set up a simple Actix Web server with `snapfire`.
//!
//! ```rust,no_run
//! use actix_web::{web, App, HttpServer, Responder};
//! use snapfire::{TeraWeb, Template};
//! use tera::Context;
//!
//! // An Actix handler that renders a template.
//! async fn index(app_state: web::Data<TeraWeb>) -> impl Responder {
//!   let mut context = Context::new();
//!   context.insert("page_title", "Welcome");
//!   // The `render` method returns a `Template` struct which is a Responder.
//!   app_state.render("index.html", context)
//! }
//!
//! #[actix_web::main]
//! async fn main() -> std::io::Result<()> {
//!   // Build the SnapFire app state.
//!   let app_state = TeraWeb::builder("templates/**/*.html")
//!     .add_global("site_name", "My Awesome Site")
//!     // In dev mode, also watch the static directory for CSS changes.
//!     .watch_static("static")
//!     .build()
//!     .expect("Failed to build TeraWeb app");
//!
//!   HttpServer::new(move || {
//!     App::new()
//!       .app_data(web::Data::new(app_state.clone()))
//!       // The middleware injects the reload script in dev mode.
//!       .wrap(snapfire::actix::dev::InjectSnapFireScript::default())
//!       .route("/", web::get().to(index))
//!       // The configure method adds the WebSocket route in dev mode.
//!       .configure(|cfg| app_state.configure_routes(cfg))
//!   })
//!   .bind(("127.0.0.1", 3000))?
//!   .run()
//!   .await
//! }
//! ```
//!
//! ### Production Builds
//!
//! To build your application for production, use the `--no-default-features` flag
//! to disable the `devel` feature:
//!
//! ```sh
//! cargo build --release --no-default-features
//! ```

pub mod actix;
pub mod core;
pub mod error;

pub use crate::core::app::{Template, TeraWeb, TeraWebBuilder};
pub use crate::error::{Result, SnapFireError};
