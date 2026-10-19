//! The plugin as the build sees it: a process on the other end of a pipe.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use snapfire_compiler_wire::{Hello, Kind, Options, Outcome, Request, Response, Severity, Unit, PROTOCOL};

struct Worker {
  child: Child,
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
}

impl Worker {
  fn start() -> (Self, Hello) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_snapfirec-svelte"))
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .spawn()
      .expect("the plugin spawns");
    let stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut line = String::new();
    stdout.read_line(&mut line).expect("the greeting arrives");
    let hello: Hello = serde_json::from_str(&line).expect("the greeting parses");
    (Self { child, stdin: Some(stdin), stdout }, hello)
  }

  fn ask(&mut self, id: u64, units: Vec<Unit>) -> Response {
    self.ask_for(id, Kind::Compile, units)
  }

  fn ask_for(&mut self, id: u64, kind: Kind, units: Vec<Unit>) -> Response {
    let request = serde_json::to_string(&Request { id, kind, units }).expect("encodes");
    let stdin = self.stdin.as_mut().expect("the pipe is open");
    writeln!(stdin, "{request}").expect("writes");
    stdin.flush().expect("flushes");
    let mut line = String::new();
    self.stdout.read_line(&mut line).expect("an answer arrives");
    serde_json::from_str(&line).expect("the answer parses")
  }
}

impl Drop for Worker {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

fn unit(name: &str, source: &str) -> Unit {
  Unit {
    filename: name.to_owned(),
    path: format!("/nowhere/{name}"),
    source: source.to_owned(),
    options: Options::default(),
    files: Default::default(),
  }
}

const CARD: &str = "<script lang=\"ts\">\nlet { title }: { title: string } = $props();\n</script>\n<h2 class=\"t\">{title}</h2>\n<style>.t { color: red }</style>\n";

#[test]
fn it_announces_itself_before_it_is_asked_anything() {
  let (_worker, hello) = Worker::start();
  assert_eq!(hello.protocol, PROTOCOL);
  assert_eq!(hello.name, "svelte");
  assert_eq!(hello.extensions, vec![".svelte".to_owned(), ".svelte.ts".to_owned(), ".svelte.js".to_owned()]);
  assert_eq!(hello.compiler, "svelte 5.57.2", "it says what it carries");
}

#[test]
fn one_worker_answers_batch_after_batch() {
  let (mut worker, _) = Worker::start();

  let first = worker.ask(1, vec![unit("a.svelte", CARD), unit("b.svelte", CARD)]);
  assert_eq!(first.id, 1);
  assert_eq!(first.results.len(), 2, "one result per unit, in order");

  let second = worker.ask(2, vec![unit("c.svelte", CARD)]);
  assert_eq!(second.id, 2, "the same process answered the second batch");
  assert_eq!(second.results.len(), 1);
}

#[test]
fn a_batch_answers_one_result_per_unit_whether_or_not_each_compiled() {
  let (mut worker, _) = Worker::start();
  let response = worker.ask(7, vec![unit("good.svelte", CARD), unit("bad.svelte", "<div>{#if open}</div>\n"), unit("also-good.svelte", CARD)]);
  assert_eq!(response.results.len(), 3, "a refusal does not shorten the batch");

  let statuses: Vec<&str> = response
    .results
    .iter()
    .map(|r| match r {
      Outcome::Ok(_) => "ok",
      Outcome::Described(_) => "described",
      Outcome::Failed { .. } => "failed",
      Outcome::Needs { .. } => "needs",
    })
    .collect();
  assert_eq!(statuses, ["ok", "failed", "ok"], "one bad component does not take its neighbours");

  let Outcome::Failed { diagnostics } = &response.results[1] else { panic!("the middle one failed") };
  assert_eq!(diagnostics[0].file.as_deref(), Some("bad.svelte"), "the diagnostic names the file the host asked about");
}

#[test]
fn closing_stdin_ends_it() {
  let (mut worker, _) = Worker::start();
  worker.ask(1, vec![unit("a.svelte", CARD)]);
  drop(worker.stdin.take());
  let status = worker.child.wait().expect("it exits");
  assert!(status.success(), "a closed pipe is how a build ends, not a failure");
}

#[test]
fn a_describe_request_answers_the_script_and_the_error_that_keeps_the_component_foreign() {
  let (mut worker, _) = Worker::start();
  let response = worker.ask_for(3, Kind::Describe, vec![unit("card.svelte", CARD)]);
  let Outcome::Described(described) = &response.results[0] else { panic!("{:?}", response.results[0]) };
  assert!(described.script.as_ref().is_some_and(|script| script.content.contains("$props()")));
  assert_eq!(described.diagnostics[0].severity, Severity::Error);
  assert_eq!(described.diagnostics[0].file.as_deref(), Some("card.svelte"));
}
