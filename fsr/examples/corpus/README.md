# corpus

The code examples of the React and Svelte documentation, built through FSR one at a time. Each example under `react_dev/` becomes an application whose only page renders its `App`; each under `svelte_dev/` one whose only page places its `App.svelte` as an island. `cargo test -p corpus` holds what the build made of it against `outcomes.txt`, one of three outcomes: `lowered`; `client:` with the causes that sent a component to the browser; `refused:` with the error that stopped the build.

The examples were written without FSR in mind. A cause that recurs is an idiom the lowerer may be missing; once read, each one either becomes a lowering with a test of its own or is kept as a refusal that is right.

```sh
cargo test -p corpus
CORPUS_BLESS=1 cargo test -p corpus   # takes this run's outcomes into outcomes.txt and svelte_outcomes.txt
```

The Svelte corpus needs `snapfirec-svelte` on `PATH`; it fetches Svelte's compiler once for the first example.

A change to the lowerer that moves an outcome fails the test, naming each example whose outcome changed; bless the run once the change is the one intended.

| Path | What it is |
| --- | --- |
| `react_dev/` | 641 examples from react.dev, see [NOTICE.md](NOTICE.md) |
| `template/` | the configuration, `app.toml`, and the import map every example is built with |
| `outcomes.txt` | one line per example, the outcome it is held to |
| `tests/corpus.rs` | the build of every example and the comparison |
| `svelte_dev/` | 166 examples and tutorial solutions from svelte.dev, see [NOTICE.md](NOTICE.md) |
| `template_svelte/` | the configuration and the import map every Svelte example is built with, Svelte 5.57.2 vendored |
| `svelte_outcomes.txt` | one line per Svelte example, the outcome it is held to |
| `tests/svelte_dev.rs` | the build of every Svelte example and the comparison |
