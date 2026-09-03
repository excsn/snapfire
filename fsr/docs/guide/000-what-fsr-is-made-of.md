# 000. What fsr is made of

The question this chapter answers: what is fsr, who is it for, what does it refuse to do and what do its words mean in the words you already have?

**For:** everyone.

## Two languages, one application

An fsr application is written in TypeScript and run by Rust. That sentence carries the whole design, so it is worth reading slowly.

The TypeScript is the application: the routes, the data each page needs, the actions a user can take, the pages and the components they are built from. It lives under `app/` and it is the thing a team changes every day. The Rust is the runtime: matching the request, fetching the data, rendering the page, holding the session, talking to the services. It lives in crates and it is the thing a team changes when the platform needs to grow. A platform developer can take any single piece of the application back into Rust without the rest noticing, which [chapter 201](201-graduating-to-rust.md) is about, but that is the exception rather than the path.

What makes this different from a Node framework is what is missing. There is no JavaScript engine in the serving path. There is no `node_modules`. There is no `npm install`. The TypeScript is read at build time, understood and turned into data the Rust runtime executes directly. A loader that fetches products becomes a row in a plan file; a page becomes a render tree; an action becomes a small program the interpreter runs. The build tells you by line when it could not do that and what happens instead.

## The two artifacts

Two files are the truth of an application and everything else is a projection of them.

The **contract** is the typed description of every service the application talks to and every schema it declares. It is imported from what the services already publish, an OpenAPI document or a `.proto` file, plus the interfaces the application writes under `schemas/`. TypeScript types are generated from it for the editor. The runtime checks every call's arguments and every response against it. Nobody writes a client, which [chapter 001](001-one-contract-no-client-code.md) explains.

The **plan file** is what the build makes of `app/`: the routes, the lowered bodies, the declared actions and the render trees, in one JSON file under `generated/`. The host reads it at boot and binds every row to something that answers it. Rust can take a row over, but the file always says who answers. The boot report prints it.

Neither TypeScript nor Rust is the source of truth for the boundary between them. The artifacts are. That is the sentence that keeps the two halves from drifting.

## What fsr refuses to do

**It will not run JavaScript on the server.** A body the build cannot understand is residue; the report names the line. What happens to residue is a decision the report makes visible rather than something that happens quietly. Today residue in a loader or action stops the build; residue in a component means that component renders in the browser only. [Chapter 002](002-a-body-is-data.md) is the argument.

**It will not let you write a client.** Services are described, imported and called by name through one typed registry. The transport, HTTP or gRPC, is the host's business.

**It will not fetch from the internet at runtime.** Browser dependencies are vendored into the repository by [`fsr add`](301-dependencies-without-npm.md) and served from disk. A checkout runs with no network and no package manager.

**It will not decide policy for you.** Which sessions expire, who may act, what an error page says: the mechanisms are there and the defaults are deliberately plain. The report tells you what was inferred so nothing decided is invisible.

## The vocabulary map

fsr's words are short and specific. Here is each one beside what other frameworks call it.

| fsr says | Meaning | Elsewhere |
| --- | --- | --- |
| body | a loader or an action, the code that runs on the server for a route | loader, server action, API handler |
| source | the named data a route's loader produces | loader data, `getServerSideProps` |
| action | a named mutation the browser can call, declared and typed | server action, form action, mutation |
| lowered | a body the build turned into data the Rust runtime executes | compiled, "runs on the edge" |
| residue | a body or component the build could not lower, named by line | "needs a runtime" |
| contract | the typed description of every service and schema | OpenAPI, the API client's types |
| plan file | the build's output: routes, bodies, actions and render trees | the route manifest |
| page | a route's component, mounted in the browser | page component |
| island | a component the browser mounts, with or without server markup over it | island, client component |
| shell | the document around every page: head, import map, entry script | root layout, `_document` |
| slot | where a child's content lands in its parent | outlet, `children` |
| segment | one region of a page with an identity, so navigation can keep or replace it | route segment |
| the report | what the build and the host print: every name and who answers it | nothing quite like it |

The word to remember is **lowered**. It is not compilation in the sense of emitting code; it is reading a body and finding it to be data. A loader that awaits two service calls and returns an object is two rows and a shape. The interpreter does not run your TypeScript, it runs the rows, which is why there is no engine and why the same body reads the same in Rust.

## How to read this guide

Each chapter opens with the question it answers and who it is for and ends with a lab: something to do to the running example that makes the chapter's claim observable. The example is [`shopping_react_ts`](../../examples/shopping_react_ts/), a storefront with a catalog, a cart and two backends it does not own, one over HTTP and one over gRPC. Have it running before you start, which its README makes a four-command job.

The labs mostly use the report. The build and the host print the same table, every route, source, action, rendered module and service with who answers it. Most of what this guide claims can be checked by changing something and reading the table again.

## The lab

Run the example and read the boot report from top to bottom. Every row is a name the application declared and the thing that answers it. Then open `app/generated/plan.json` and find the same names. The report is the plan file with the host's decisions added; nothing on the screen is not in one of the two artifacts.
