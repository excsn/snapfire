use std::path::Path;
use std::sync::Arc;
use std::time::Duration;


#[tokio::main]
async fn main() -> std::io::Result<()> {
  // Events out to fibre_logging's appenders, spans kept for `/__fsr/traces`.
  let logging = Path::new(env!("CARGO_MANIFEST_DIR")).join("fibre_logging.yaml");
  let (traces, _logging, why) = snapfire_fsr_host::trace::observe(&logging);
  if let Some(why) = why {
    eprintln!("logging disabled: {why}");
  }
  let host = Arc::new(
    advanced_tera_app::builder(Duration::from_millis(1500))
      .and_then(|builder| builder.traces(traces).build())
      .map_err(std::io::Error::other)?,
  );
  print!("{}", host.report());
  println!("advanced_tera_app on http://{}/dash/servers and /slow/servers", host.listen());
  let listen = host.listen().to_owned();
  host.serve(&listen).await
}
