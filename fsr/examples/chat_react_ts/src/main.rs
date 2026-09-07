use std::sync::Arc;

use chat_react_ts::backend;
use snapfire_fsr_core::Value;
use snapfire_fsr_host::Host;

/// Rooms, over the seam that pushes. Saying something is an action; the
/// backend keeps it and names the room's topic; the host publishes that topic
/// and every page following the room revalidates, so a message someone else
/// sent lands without a reload and without polling.
///
/// A topic is not public: `room/<id>` is followed only by a session that has
/// opened that room, which the room's loader records. Anything else is 403.
#[tokio::main]
async fn main() -> std::io::Result<()> {
  let (transport, rooms) = backend::rooms();
  let host = Host::from(env!("CARGO_MANIFEST_DIR"))
    .map(|builder| {
      builder.services_over(transport).native("digest", Arc::new(chat_react_ts::native::Digest)).topics(|topic, session, _| match topic.strip_prefix("room/") {
        Some(room) => matches!(session.get("rooms"), Some(Value::Map(open)) if open.contains_key(room)),
        None => false,
      })
    })
    .and_then(|builder| builder.build())
    .map_err(std::io::Error::other)?;
  let host = Arc::new(host);
  print!("{}", host.report());

  let publishing = host.clone();
  let mut changes = rooms.changes();
  tokio::spawn(async move {
    while let Ok(topic) = changes.recv().await {
      publishing.publish(topic);
    }
  });

  let listen = host.listen().to_owned();
  println!("rooms on http://{listen}/");
  println!("open the same room in two windows and say something in one");
  host.serve(&listen).await
}
