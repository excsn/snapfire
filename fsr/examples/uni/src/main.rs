use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let ticks = uni::state::Ticks::default();
  let host = Arc::new(uni::builder(ticks.clone()).and_then(|b| b.build()).map_err(std::io::Error::other)?);
  print!("{}", host.report());
  println!("uni on http://{}/board", host.listen());

  // The desk's own clock: every tick moves the prices and tells whoever is
  // watching, so a page follows the server without asking it anything.
  let publisher = host.clone();
  tokio::spawn(async move {
    loop {
      tokio::time::sleep(Duration::from_secs(4)).await;
      ticks.advance();
      publisher.publish("prices");
    }
  });

  let listen = host.listen().to_owned();
  host.serve(&listen).await
}
