//! `loadgen <url> <concurrency> <seconds>`: one run against a server already up, printed as one JSON line.

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
  let args: Vec<String> = std::env::args().collect();
  if args.len() != 4 {
    eprintln!("usage: loadgen <url> <concurrency> <seconds>");
    std::process::exit(2);
  }
  let concurrency: usize = args[2].parse().expect("a concurrency");
  let seconds: u64 = args[3].parse().expect("seconds");
  match load::run(&args[1], concurrency, seconds).await {
    Ok(run) => {
      for why in &run.errors {
        eprintln!("{why}");
      }
      println!("{}", run.json());
    }
    Err(why) => {
      eprintln!("{why}");
      std::process::exit(1);
    }
  }
}
