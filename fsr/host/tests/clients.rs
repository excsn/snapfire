#![cfg(feature = "http")]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_host::Host;
use snapfire_fsr_runtime::FailureKind;

const SHOP: &str = r#"{"openapi":"3.0.0","info":{"title":"shop","version":"1"},"paths":{"/items":{"get":{"operationId":"list","responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"type":"array","items":{"type":"string"}}}}}}}}}}"#;

fn project(tag: &str, client: &str) -> PathBuf {
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-host-clients-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::create_dir_all(root.join("app/clients")).unwrap();
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), "(plan 6)\n").unwrap();
  std::fs::write(root.join("app/clients/shop.openapi.json"), SHOP).unwrap();
  std::fs::write(root.join("config/app.toml"), format!("[server]\nlisten = \"127.0.0.1:0\"\n[document]\ntitle = \"Clients\"\n[session]\nkey = \"k\"\n[clients.shop]\n{client}")).unwrap();
  root
}

/// Answers each request with `["a"]` and records its `user-agent` header.
fn recording_backend(requests: usize) -> (String, Arc<Mutex<Vec<Option<String>>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let seen = Arc::new(Mutex::new(Vec::new()));
  let recorder = seen.clone();
  std::thread::spawn(move || {
    for _ in 0..requests {
      let Ok((stream, _)) = listener.accept() else { return };
      let mut reader = BufReader::new(stream);
      let mut agent = None;
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
          break;
        }
        if let Some((name, value)) = line.split_once(':') {
          if name.eq_ignore_ascii_case("user-agent") {
            agent = Some(value.trim().to_owned());
          }
        }
      }
      recorder.lock().push(agent);
      let mut stream = reader.into_inner();
      let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 5\r\nconnection: close\r\n\r\n[\"a\"]");
    }
  });
  (base, seen)
}

#[tokio::test]
async fn a_client_sends_the_user_agent_its_section_names_and_the_transport_s_own_without_one() {
  let (base, seen) = recording_backend(2);
  for (tag, extra) in [("named", "user_agent = \"snapfire-www\"\n"), ("default", "")] {
    let root = project(tag, &format!("base_url = \"{base}\"\n{extra}"));
    let host = Host::from(&root).unwrap().build().unwrap();
    host.services().bind_anonymous().call("shop", "list", ValueMap::default()).await.unwrap();
    std::fs::remove_dir_all(&root).unwrap();
  }
  let seen = seen.lock();
  assert_eq!(seen[0].as_deref(), Some("snapfire-www"));
  assert!(seen[1].as_deref().is_some_and(|agent| agent.starts_with("snapfire_fsr_service/")), "{:?}", seen[1]);
}

#[tokio::test]
async fn a_client_whose_backend_answers_slower_than_its_timeout_fails_as_a_timeout() {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  std::thread::spawn(move || {
    let held: Vec<_> = listener.incoming().take(1).collect();
    std::thread::sleep(Duration::from_secs(10));
    drop(held);
  });
  let root = project("timeout", &format!("base_url = \"{base}\"\ntimeout = \"300ms\"\n"));
  let host = Host::from(&root).unwrap().build().unwrap();
  std::fs::remove_dir_all(&root).unwrap();

  let started = Instant::now();
  let failed = host.services().bind_anonymous().call("shop", "list", ValueMap::default()).await.unwrap_err();
  assert_eq!(failed.kind, FailureKind::Timeout, "{failed:?}");
  assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}

#[test]
fn a_client_timeout_that_is_not_a_duration_is_refused_with_its_key() {
  let root = project("bad_timeout", "base_url = \"http://127.0.0.1:1\"\ntimeout = \"soon\"\n");
  let built = Host::from(&root).and_then(|host| host.build());
  std::fs::remove_dir_all(&root).unwrap();
  let error = built.err().expect("a timeout that is not a duration is refused").to_string();
  assert!(error.contains("clients.shop.timeout"), "{error}");
}
