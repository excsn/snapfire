use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let host = Arc::new(uni::build().map_err(std::io::Error::other)?);
  print!("{}", host.report());
  println!("uni on http://{}/board", host.listen());
  let listen = host.listen().to_owned();
  host.serve(&listen).await
}
