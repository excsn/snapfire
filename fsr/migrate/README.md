# snapfire_fsr_migrate

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_migrate.svg)](https://crates.io/crates/snapfire_fsr_migrate)
[![Docs.rs](https://docs.rs/snapfire_fsr_migrate/badge.svg)](https://docs.rs/snapfire_fsr_migrate)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Rewrites an application written for Next.js, SvelteKit, Nuxt or Ink into a SnapFire FSR application, and names each line it left for a person.

It moves routes, layouts and API handlers into FSR's `app/routes/` layout. It turns data fetching into route loaders and head tags into a loader's `meta`. It carries public environment variables, scripts and ESLint settings across. Whatever it cannot rewrite stays as it was, with a note giving the file and line and saying why.

Most applications never depend on this crate. `fsr migrate` is built on it:

```sh
fsr migrate ./my-next-app ./my-fsr-app
```

Use the crate directly when you build your own tooling around migration.

## What it reads

| Framework | Detected by | Components rendered with |
| --- | --- | --- |
| Next.js App Router | `next` in `package.json` and an `app/` or `src/app/` directory | React |
| Next.js Pages Router | `next` in `package.json` and only a `pages/` or `src/pages/` directory | React |
| SvelteKit | `@sveltejs/kit` in `package.json` | Svelte |
| Nuxt | `nuxt` in `package.json` | Vue |
| Ink | `ink` in `package.json` | React, as a terminal app |

## Using the crate

Nothing here touches the disk. The input and the output are `Tree`s of files by path, so a caller reads a directory into one and writes the result back out.

```rust
use snapfire_fsr_migrate::{migrate, Tree};

let mut input = Tree::new();
input.insert("package.json", r#"{ "dependencies": { "next": "latest" } }"#);
input.insert("app/page.tsx", "export default function Page() {\n  return <main>Hello</main>;\n}\n");

let (output, report) = migrate(&input);
for path in output.paths() {
  println!("wrote {path}");
}
for note in &report.notes {
  println!("note {}:{}: {}", note.file, note.line, note.message);
}
for command in &report.commands {
  println!("next {command}");
}
```

The `Report` says which framework it detected and which passes ran. It lists the notes, the packages the output still imports with the version ranges `package.json` asked for, the commands to run in the migrated project and the tables to add to `config/app.toml`.

`lift` runs the passes that apply to any FSR application on their own, such as moving what a component awaits while it renders into the route's loader. `relayout` moves an FSR project from the old client imports onto `@snapfire/fsr/client` and `@snapfire/fsr/host`.

## Limits

It reads ES modules only. When a page, a layout, a route handler or a file they import is CommonJS, nothing is migrated: `report.refusals` names each such file and the line that shows it. `next.config.js` and a server entry beside the application are not migrated, so CommonJS there is not refused.

A migrated application still has to build. `fsr check` builds and typechecks it, and that is where the build names any component it leaves to the browser.

## More information

* [Moving an application from Next.js](https://github.com/excsn/snapfire/blob/main/fsr/docs/guide/112-moving-an-application-from-nextjs.md) walks through the command, where each file goes and what to run next.
* [`fsr migrate` in the CLI](https://github.com/excsn/snapfire/blob/main/fsr/cli/README.md) lists the command beside the rest of `fsr`.

## License

MPL-2.0.
