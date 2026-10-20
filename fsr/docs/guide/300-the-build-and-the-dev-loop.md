# 300. The build and the dev loop

The question this chapter answers: what does `fsr build` produce, why is none of it committed and what does `fsr dev` do when a file changes?

**For:** everyone.

## What `fsr build` writes

`fsr build <app>` walks `app/` and writes `app/generated/`:

- **The plan file**, `generated/plan.sexp`: routes, lowered bodies, declared actions and render trees. The host reads it at boot.
- **The contracts**, one file per client document plus one for the schemas, under `generated/contracts/`. The host merges them at boot.
- **The TypeScript the application is written against**: `fsr.ts` with `Ctx` and `ActionCtx` per route, `services.d.ts` from the contract, `client.ts` with every page's props type and every action's typed callable, `islands.ts` registering every page for the browser, `testing.ts` for the tests.
- **Both tsconfigs**: `tsconfig.json` for the editor and `tsc`, with the aliases and the type roots, plus `tsconfig.build.json` for the bundle.
- **The asset manifest**, `generated/assets.json`: every image a component imported with its size and variants, every face under `fonts/` with the CSS that serves it. The host reads it at boot; the variants and the font copies themselves are written under `dist/` once the bundle has run, each named with its hash so a rebuild with nothing changed writes nothing. Chapter 108 has the rest.

`fsr check <app>` does the same without writing, prints the report and exits non-zero when anything is residue. It is the command for a pull request.

Nothing under `generated/` is committed. It is rebuilt from `routes/`, `schemas/` and `clients/` in well under a second; committing it would mean every branch that touches a loader conflicts with every other on a JSON file nobody edits by hand. The example's `.gitignore` says which directories those are; a new application copies it.

## The bundle is a separate tool

snapfirec compiles the browser side into `dist/`, following `tsconfig.build.json`: the files the browser loads by name and what they import. Those are the islands, the elements an island defines, the route modules the browser mounts, `src/main` and the two generated modules the browser needs. It is a general TypeScript compiler that knows nothing about fsr; the build writes the tsconfig it needs and `fsr dev` runs it. The bundle must follow the build, since it compiles the island registry the build writes, which is the one ordering mistake a fresh checkout can make and the reason `fsr dev` exists.

Not every route module is in it. A template the `--verbose` report marks `static`, one with no state and no handlers of its own, has no browser twin, so the build lists the route files that do hydrate in `tsconfig.build.json` rather than `routes/**/*.tsx`. A page or a layout is static once the build has moved its state into an island beside it. That island is listed under `files`, since it exists only in the bundle overlay. A component under `src/` that only templates render is the same: nothing in the browser loads it, so it is not compiled either. An application whose templates are all static bundles its entry, its elements and the two generated files and nothing else. A `page.tera` or `layout.tera` stays outside the build entirely: the report lists it as `template`, nothing is lowered, bundled or typechecked for it and the host renders it from the file, while the island modules it places by literal are bundled like any other.

A `.vue` file under `src/` is compiled by a plugin, `snapfirec-vue` on `PATH`, which snapfirec spawns once and keeps for the build; the file becomes a module and a scoped stylesheet beside it. The build's banner names the plugin and says how many of its files the plugin cache answered. Chapter 104 covers it in full.

The bundle's own facts file, `dist/.snapfire-build.json`, is what the host reads to infer the public path, the entry script and the component stylesheets to link, so the two tools share a file instead of configuration.

They also share a second directory. For every component whose render-path calls or static subtrees the server computes for the browser, the build writes a copy of the module under `app/.fsr/bundle/` with those calls turned into reads of what the server delivered and `fsr dev` passes `--overlay .fsr/bundle` so snapfirec compiles the copy in place of the source at the same path. The source is never touched, the editor and `fsr check` never see the copy and the directory is rebuilt on every build, so a component that stops qualifying stops being overlaid. It is not committed either; the example's `.gitignore` lists it beside `generated/`.

## A Cargo project runs the build for you

An application with a Rust project puts three lines in `build.rs` that call `build` and then `write`. `cargo build` then regenerates `generated/` whenever `routes/`, `schemas/` or `clients/` change, so the host never boots against a stale plan. The storefront does this, which is why its README's manual steps are `cargo build`, the bundle, `cargo run`, in that order.

## `fsr dev`

`fsr dev <app>` is those steps in a loop. It generates, bundles, builds the Cargo project beside the app and runs it or runs the stock host when there is no such project, then watches. What it does on a change depends on what changed:

| Changed | It does |
| --- | --- |
| a page, a component or CSS | rebundles; the open page refreshes itself |
| a `.vue` file or a file a `<style src>` in one names | recompiles that component through the plugin, then rebundles |
| a loader, an action, a schema or a client document | regenerates, rebundles, reloads the running server in place |
| Rust under `src/`, `build.rs`, `Cargo.toml` or `config/` | rebuilds, restarts |
| `config/` beside an app with no Cargo project | reloads the stock host in place, restarts when refused |
| a test | nothing; `fsr test` is its own command |

On a site, the loop serves the site under its shell: the shell `[site] shell` points into or the one `--under <shell>` names is served as it stands with the site mounted from its tree. The site alone is built and watched. `--alone` runs the site by itself. Chapter 205 has the shape.

The browser follows along. A served document in development carries a script listening on the host's `/__fsr/events`; the loop tells the host after a rebundle and the host tells every open document. A stylesheet edit re-links the stylesheets and refreshes the route's payload in place, so the layout keeps its state. A sheet added to or removed from `styles/` is linked or unlinked in place the same way. Any larger change to the list of sheets reloads the page; an edited module reloads the page, since the one it hydrated with is stale; a restart drops the stream and the reconnect does the same check. `server.dev = false` turns it off and `prerender` never writes the script.

The same script lists each Content-Security-Policy violation the page meets in a panel, with the key under `[document.csp]` or `[document.csp_report_only]` that would admit it. A violation from a browser extension is counted rather than listed. Chapter 305 covers the policy itself.

When the generated files actually differ the loop asks the running server to reload its tables in place (`POST /__fsr/reload`) and restarts only when the reload is refused (a changed `[session]`, for one) or when the Rust project changed. Editing a page never restarts, because the bundle's output names are stable and the host reads it from disk; a reload keeps every session, which chapter 204 explains. A step that fails leaves the running server up and waits, so a typo never takes the page down. Stopping the loop stops the server with it.

Without a Cargo project beside the app, the loop runs `fsr serve` on the app instead: the stock host over `config/app.toml`, restarted on the same rule, with `config/` watched in place of `src/`. `fsr serve <app> [--listen <addr>]` is that host on its own, for production, the way `cargo run` is for a project that has one; it refuses a configuration whose `[app] dir` names a different directory. It prints the boot report's rows that need attention and `--verbose` prints the whole report, which `fsr dev --verbose` passes on. An `fsr` built with the `desktop` feature takes `--desktop` on both: the loop or the host runs beside a native window over the system webview, which loads the host over loopback and keeps its window across the loop's restarts.

## Prerendering

A route whose loader reads no parameter, no query, no session, no identity and no clock renders the same for every request. The host can tell, because the loader is data it can read. The boot report lists such routes under `prerender`. `fsr prerender app` renders each once into `server.prerender`, as does the project's binary with `--prerender`. A production host answers those routes from the files with `x-sf-prerendered: 1` so you can see it. A relative `server.prerender` is resolved against the app directory, so `dist/prerender` in a project-root config lands under `app/`.

```sh
fsr prerender app
fsr bundle app --out deploy/
```

The storefront's `/about` qualifies; every other page reads the cart from the session, so none of them does. There is nothing to declare: a loader that starts reading the session takes its route back to per-request rendering at the next build. A route with a parameter is the one exception, since nothing can tell which values it takes: its page loader exports `paths` naming them and the command writes one file per set, `/blog/hello` and `/blog/world` for `/blog/{slug}`. Each run lists what it wrote in `prerendered.json` and removes the files of a set that dropped out, so no stale page is left. A page with a generated card image, chapter 108's `opengraph-image.tsx`, gets the image written beside its document too, `opengraph-image.png` or one file per id a module lists; production answers the image from that file.

A route the host cannot write whole still gets what it can: under a layout reading the session, a page reading nothing is rendered once by the same command into `renders.json` and spliced in at request time, so only the layout is evaluated per request.

## A prerender belongs to its build

Each run also writes `prerender.build`: a hash of the plan, the templates and the assets manifest it rendered from. A host whose own build hashes differently does not read the directory. It renders those routes live and its boot report says why:

```text
prerender /about                 app/dist/prerender written by another build, rendered live until fsr prerender runs
```

So a deploy that ships a new build with a prerender directory from an older one serves the new pages, not the old files. Running `fsr prerender` from the new build puts the files back in use. The configuration is left out of the hash, since a directory prerendered on a development machine is served under production's configuration.

## Previewing a prerendered page in development

In development the host renders every page live and never answers from the directory on its own. An edit to a page and a change in what its loader returns both show on the next request.

Next.js, Astro, SvelteKit and Nuxt also render live in development. The only way to see their prerendered output is a production build served locally, `next build` then `next start`. Here you can see any page as a prerender would write it while the development server keeps running: add `?__prerendered` to its URL.

```sh
curl -i 'http://127.0.0.1:8190/notice/bins?__prerendered'
```

The host renders that page the way `fsr prerender` does: anonymously, with its loaders run now and the head a written file carries. It answers with `x-sf-prerendered: preview`. Nothing is read from the directory and nothing is written to it, so a project that has never run `fsr prerender` previews its pages the same way and an edit shows in the preview on the next request. `?__prerendered&__payload` previews the payload a navigation would read from the file. A page that would have no file answers `404` saying why: the route reads the request, the path is not one its `paths` lists or its loader failed. A production host answers from the files whatever the query says.

## Prerendering again while the host serves

A page prerendered from a service's answers shows what the service said when it ran. To refresh it without a redeploy, call `Host::prerender` from the application's own Rust, on whatever schedule suits the data:

```rust
let host = Arc::new(builder.build()?);
let out = host.report().prerender.clone().expect("server.prerender is set");
let refreshing = host.clone();
tokio::spawn(async move {
  let mut every = tokio::time::interval(Duration::from_secs(600));
  loop {
    every.tick().await;
    if let Err(e) = refreshing.prerender(&out).await {
      log::warn!("prerender: {e}");
    }
  }
});
```

The host never runs this itself. A run is safe beside the requests it serves: each file is written aside and renamed into place, so a request gets the old page or the new one and never half of one. A page whose loader fails this time keeps the file the last run wrote. Once every page is written, the files for paths that dropped out of `paths` are removed.

## The lab

Run `fsr dev --verbose app` in the storefront, open a product in the browser and type into the header's search box. Add `h1 { letter-spacing: 3px; }` to `styles/app.css` and save: the heading spreads out and the search text is still there, since only the stylesheet and the payload moved. Now edit the button text in `routes/product/[id]/page.tsx` and save: the log shows the report again because the plan changed, the server reloads its tables in place without restarting and the page reloads with the new text. Now edit `routes/page.loader.ts`, remove `tag: query.tag` from the call and save: the log shows the report again and the server reloads in place, again with no restart. Put it back.

Then break something: add `try {` without a closing brace to the loader. The loop prints the parse error with its line and waits; the previous server keeps serving. Fix it and the loop picks up where it left off.
