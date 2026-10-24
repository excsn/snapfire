//! A lock held the same way by threads and by processes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::lock::{exclusive, once, OnceError};

fn dir(tag: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr-lock-{tag}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

#[test]
fn an_exclusive_lock_lets_one_holder_in_at_a_time() {
  let dir = dir("exclusive");
  let inside = Arc::new(AtomicUsize::new(0));
  let most = Arc::new(AtomicUsize::new(0));
  let workers: Vec<_> = (0..8)
    .map(|_| {
      let (dir, inside, most) = (dir.clone(), inside.clone(), most.clone());
      std::thread::spawn(move || {
        let _held = exclusive(dir.join("one.lock")).unwrap();
        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
        most.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(5));
        inside.fetch_sub(1, Ordering::SeqCst);
      })
    })
    .collect();
  workers.into_iter().for_each(|w| w.join().unwrap());
  assert_eq!(most.load(Ordering::SeqCst), 1);
}

#[test]
fn once_runs_its_setup_one_time_and_again_after_a_failure() {
  let dir = dir("once");
  let ran = Arc::new(AtomicUsize::new(0));
  let workers: Vec<_> = (0..8)
    .map(|_| {
      let (dir, ran) = (dir.clone(), ran.clone());
      std::thread::spawn(move || once(&dir, "deploy", || -> Result<(), String> {
        ran.fetch_add(1, Ordering::SeqCst);
        Ok(())
      }).unwrap())
    })
    .collect();
  let ran_here: usize = workers.into_iter().map(|w| usize::from(w.join().unwrap())).sum();
  assert_eq!((ran.load(Ordering::SeqCst), ran_here), (1, 1));

  let failed = once(&dir, "flaky", || Err("down".to_owned()));
  assert!(matches!(failed, Err(OnceError::Setup(ref why)) if why == "down"));
  assert_eq!(once(&dir, "flaky", || Ok::<(), String>(())).unwrap(), true, "a failed setup recorded nothing");
}
