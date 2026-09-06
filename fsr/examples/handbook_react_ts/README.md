# handbook_react_ts

A documentation site with no server. Three routes, no backend, nothing on any plan that reads the request, so every route is fixed and the whole site is files: `cargo run -p handbook_react_ts` builds the host, writes each route to `app/dist/prerender` and exits. What serves the directory afterwards is not this program's business.

It exists to prove that a different deployment is a different build of the same framework rather than a different framework: the routes, the loaders, the layout and the islands are written exactly as the storefront's are.

## Running it

```sh
cargo run -p handbook_react_ts          # writes site/ and exits
cd site && python3 -m http.server 8110
```

`site/` is the whole thing: a document and a payload per route, plus every static root the configuration names copied in, so the client bundle, the vendored React and the stylesheet sit where the documents ask for them. It is written beside the app rather than under it, since one of those roots is `app/dist` itself.

`fsr dev app` is still the authoring loop, with the live refresh and the rebuild on save. The generator is what you run when the writing is done.

## What it holds

| Path | What it is |
| --- | --- |
| `/` | three cards from a loader that reads a module constant |
| `/install` | the commands, in order |
| `/faq` | questions, including this one |

Every page's data comes from `app/src/content.ts` through an ordinary `page.loader.ts`. A loader that calls no service is still a loader: it lowers, it runs in Rust and its return type is what types the page's props.

## What a static host does and does not give you

The documents are complete and self-contained: markup rendered by the server, the title the route's `meta` returned, one stylesheet and one module script, every reference under `/static` or a plain path. Nothing points at a host and no development endpoint is baked in.

Client navigation is the part that depends on what serves the files. The navigator asks for `/faq?__payload`, which is why `prerender` writes an `index.payload` beside every `index.html`. A host that maps that query to the file, which the stock host does, gives client-side navigation over a directory of files. A static host that ignores the query returns the document instead, the payload fails to parse and the navigator falls back to a full page load, so links keep working and nothing breaks. Islands still hydrate either way.

## Tests

`cargo test -p handbook_react_ts`: that every route is prerenderable and none is left out, that the whole site is written as documents and payloads, that a written document carries the page's markup, its own title and no reference to a host, and that a render with a session full of junk is byte for byte the render without one.

`fsr test app`: the home page's cards come from the loader's constant and the layout wraps every page.

Checked in a browser over `python3 -m http.server`: every document and every asset answers, the page hydrates with an empty console, and following a link lands on the next page with its own title.
