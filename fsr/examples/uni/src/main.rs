use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let ticks = uni::state::Ticks::default();
  let tape = uni::state::Tape::default();
  let host = Arc::new(uni::builder(ticks.clone(), tape.clone()).and_then(|b| b.build()).map_err(std::io::Error::other)?);
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

  // The tape's, at the interval its own poll runs on. Nothing a request does
  // moves it, so a revalidation anywhere else leaves the headlines where the
  // reader left them.
  tokio::spawn(async move {
    loop {
      tokio::time::sleep(Duration::from_secs(10)).await;
      tape.advance();
    }
  });

  let listen = host.listen().to_owned();
  host.serve(&listen).await
}
