//! The plugin as the build sees it: a process on the other end of a pipe.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use snapfire_plugin::{Hello, Options, Request, Response, Unit, PROTOCOL};

struct Worker {
  child: Child,
  /// Taken when a test wants to see what a closed pipe does.
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
}

impl Worker {
  fn start() -> (Self, Hello) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_snapfirec-vue"))
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
    let request = serde_json::to_string(&Request { id, units }).expect("encodes");
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

const CARD: &str = "<script setup lang=\"ts\">\nconst props = defineProps<{ title: string }>();\n</script>\n<template><h2 class=\"t\">{{ title }}</h2></template>\n<style scoped>.t { color: red }</style>\n";

#[test]
fn it_announces_itself_before_it_is_asked_anything() {
  let (_worker, hello) = Worker::start();
  assert_eq!(hello.protocol, PROTOCOL);
  assert_eq!(hello.name, "vue");
  assert_eq!(hello.extensions, vec![".vue".to_owned()]);
  assert!(hello.compiler.starts_with("@vue/compiler-sfc "), "it says what it carries: {}", hello.compiler);
}

#[test]
fn one_worker_answers_batch_after_batch() {
  let (mut worker, _) = Worker::start();

  let first = worker.ask(1, vec![unit("a.vue", CARD), unit("b.vue", CARD)]);
  assert_eq!(first.id, 1);
  assert_eq!(first.results.len(), 2, "one result per unit, in order");

  let second = worker.ask(2, vec![unit("c.vue", CARD)]);
  assert_eq!(second.id, 2, "the same process answered the second batch");
  assert_eq!(second.results.len(), 1);

  let third = worker.ask(3, vec![unit("d.vue", CARD)]);
  assert_eq!(third.id, 3);
}

#[test]
fn a_batch_answers_one_result_per_unit_whether_or_not_each_compiled() {
  let (mut worker, _) = Worker::start();
  let response = worker.ask(
    7,
    vec![
      unit("good.vue", CARD),
      unit("bad.vue", "<template><div v-if=\"</div></template>\n"),
      unit("also-good.vue", CARD),
    ],
  );
  assert_eq!(response.results.len(), 3, "a refusal does not shorten the batch");

  let statuses: Vec<&str> = response
    .results
    .iter()
    .map(|r| match r {
      snapfire_plugin::Outcome::Ok(_) => "ok",
      snapfire_plugin::Outcome::Failed { .. } => "failed",
      snapfire_plugin::Outcome::Needs { .. } => "needs",
    })
    .collect();
  assert_eq!(statuses, ["ok", "failed", "ok"], "one bad component does not take its neighbours");

  let snapfire_plugin::Outcome::Failed { diagnostics } = &response.results[1] else { panic!("the middle one failed") };
  assert_eq!(diagnostics[0].file.as_deref(), Some("bad.vue"), "the diagnostic names the file the host asked about");
}

#[test]
fn closing_stdin_ends_it() {
  let (mut worker, _) = Worker::start();
  worker.ask(1, vec![unit("a.vue", CARD)]);
  drop(worker.stdin.take());
  let status = worker.child.wait().expect("it exits");
  assert!(status.success(), "a closed pipe is how a build ends, not a failure");
}
