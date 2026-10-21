# corpus

The code examples of the React, Svelte and Vue documentation, built through FSR one at a time. Ink's examples run through its terminal mode as well. Each example under `react_dev/` becomes an application whose only page renders its `App`; each under `svelte_dev/` or `vuejs_org/` one whose only page places its `App.svelte` or `App.vue` as an island. `cargo test -p corpus` holds what the build made of it against `outcomes.txt`, one of three outcomes: `lowered`; `client:` with the causes that sent a component to the browser; `refused:` with the error that stopped the build. The Next.js and Nuxt examples and the SvelteKit tutorial are whole applications written for those frameworks. `cargo test -p corpus --test next_examples` migrates each Next.js one with `fsr migrate` and builds the result; the Nuxt and SvelteKit ones are not built yet.

The examples were written without FSR in mind. A cause that recurs is an idiom the lowerer may be missing; once read, each one either becomes a lowering with a test of its own or is kept as a refusal that is right.

```sh
cargo test -p corpus
CORPUS_BLESS=1 cargo test -p corpus   # takes this run's outcomes into the three outcome files
```

The Svelte corpus needs `snapfirec-svelte` on `PATH` and fetches Svelte's compiler once for the first example. The Vue corpus needs `snapfirec-vue` on `PATH`.

A change to the lowerer that moves an outcome fails the test, naming each example whose outcome changed; bless the run once the change is the one intended.

| Path | What it is |
| --- | --- |
| `react_dev/` | 641 examples from react.dev, see [NOTICE.md](NOTICE.md) |
| `template/` | the configuration, `app.toml`, and the import map every example is built with |
| `outcomes.txt` | one line per example, the outcome it is held to |
| `tests/corpus.rs` | the build of every example and the comparison |
| `svelte_dev/` | 167 examples and tutorial solutions from svelte.dev, see [NOTICE.md](NOTICE.md) |
| `template_svelte/` | the configuration and the import map every Svelte example is built with, Svelte 5.57.2 vendored |
| `svelte_outcomes.txt` | one line per Svelte example, the outcome it is held to |
| `tests/svelte_dev.rs` | the build of every Svelte example and the comparison |
| `vuejs_org/` | 70 examples and tutorial steps from vuejs.org, each in its Composition API and Options API form, see [NOTICE.md](NOTICE.md) |
| `template_vue/` | the configuration and the import map every Vue example is built with, Vue vendored |
| `vue_outcomes.txt` | one line per Vue example, the outcome it is held to |
| `tests/vuejs_org.rs` | the build of every Vue example and the comparison |
| `ink_examples/` | the 29 examples of Ink, run in the terminal, see [NOTICE.md](NOTICE.md) |
| `ink_outcomes.txt` | one line per Ink example: `rendered` or `refused:` with the reason |
| `tests/ink.rs` | the run of every Ink example and the comparison; `CORPUS_FRAMES=1` prints each first frame |
| `next_examples/` | the 231 examples of Next.js, see [NOTICE.md](NOTICE.md) |
| `next_outcomes.txt` | one line per Next.js application: what `fsr migrate` made of it and how its build went, with the pages routed and the notes left |
| `tests/next_examples.rs` | the migration and build of every Next.js application and the comparison; `CORPUS_NOTES=1` prints each note |
| `nuxt_examples/` | the 24 examples of Nuxt and the layer they extend, see [NOTICE.md](NOTICE.md) |
| `sveltekit_tutorial/` | the 46 exercises of the SvelteKit chapters of the svelte.dev tutorial, see [NOTICE.md](NOTICE.md) |
