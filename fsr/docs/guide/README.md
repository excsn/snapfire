# The fsr guide

This is the learning layer of fsr's documentation. The example's [README](../../examples/shopping_react_ts/README.md) gets a checkout running in four commands; the [ops console](../../examples/ops_console_react_ts/README.md) is the second example, built to exercise what the storefront never touches. Each crate's README and API reference say exactly what to type. This guide is for the part in between: how fsr approaches the problems of a full-stack application, why the pieces have the shape they have and where the seams are when you want to own one.

Every chapter answers one question, reads in one sitting and names a lab: something to do to the running example that makes the chapter's claim observable and usually lets you watch it fail too. The labs are not decoration. A guide you only read is a guide you will misremember.

## Who each chapter is for

fsr has two kinds of developer and the guide says which one it is talking to at the top of every chapter.

- **App developers** write TypeScript under `app/`: routes, loaders, actions, pages, components, schemas and tests. They never write Rust and never run Node.
- **Platform developers** write Rust against the host and the crates: overriding a body, adding a service transport, mounting the host somewhere else, extending the platform for a team.

A chapter marked **everyone** is one both need. Nothing in the app developer chapters requires reading the platform ones; the reverse is almost true: a platform developer should read 100 and 101 once, since the bodies they override are written there.

## The chapters

**Foundations**, or what you are standing on:

- [000. What fsr is made of](000-what-fsr-is-made-of.md), TypeScript as the application language, Rust as the runtime, the two artifacts that are the truth, what fsr refuses to do and the vocabulary map. Everyone.
- [001. One contract, no client code](001-one-contract-no-client-code.md), how a service's own document becomes a typed call nobody wrote. Everyone.
- [002. A body is data](002-a-body-is-data.md), why a loader is lowered rather than run, what residue is and why the report always says where a body runs. Everyone.
- [003. Rendered where it is cheapest](003-rendered-where-it-is-cheapest.md), how a React page is rendered on the server with no JavaScript engine, what hydrates over it and what the browser reads instead of computing again. Everyone.

**The application**, or what you write:

- [100. Routes, loaders and pages](100-routes-loaders-and-pages.md), the file conventions, params and query, the props a page receives and navigation that keeps the layout. App developers.
- [101. Actions and the session](101-actions-and-the-session.md), schemas, session defaults, guards and the cart as the worked case. App developers.
- [102. Components the server renders](102-components-the-server-renders.md), what a component may say, helpers, `useState`, what stays in the browser, what the server computes for it and an island the server drives. App developers.
- [103. Testing a body and a page](103-testing-a-body.md), mocks the contract checks, the trace, page tests over a DOM with hydration, loading a route and clicking through it, and `fsr test`. App developers.

**The host**, or what runs it:

- [200. The stock host and its configuration](200-the-stock-host-and-its-configuration.md), the config ladder, what the host infers and the boot report. Platform developers.
- [201. Graduating to Rust](201-graduating-to-rust.md), taking one name back from the plan file and the rule that keeps it honest. Platform developers.
- [202. Services and transports](202-services-and-transports.md), HTTP, gRPC, interceptors and why application code never sees a token. Platform developers.
- [203. Sessions and identity](203-sessions-and-identity.md), the signed cookie, the store, who the request is and where a login goes. Platform developers.
- [204. Reloading in place](204-reloading-in-place.md), the tables a request reads, what a reload swaps and what it refuses, and why `fsr dev` no longer restarts. Platform developers.
- [205. Sites: one product, many teams](205-sites.md), a team's application built as a site, the shell that mounts it under a path, what crosses the seam and how a deploy is a pointer moved. Everyone.

**Tooling**, or the commands:

- [300. The build and the dev loop](300-the-build-and-the-dev-loop.md), `fsr build`, `fsr check`, `fsr dev`, `fsr serve` and why `generated/` is not committed. Everyone.
- [301. Dependencies without npm](301-dependencies-without-npm.md), `fsr add`, `fsr types`, the import map and what xwpm changes. Everyone.
- [302. Imports and aliases](302-imports-and-aliases.md), the five prefixes and where each of the three readers resolves them. Everyone.

And one appendix:

- [900. The parts bin](900-the-parts-bin.md), every block in every crate, one line each, sorted by the itch it scratches.

## Reading paths

**A frontend developer who knows Next or Remix:** read 000 for the vocabulary, then 100, 101, 102, 103. Chapter 003 will read like the rendering model you already have, minus the engine.

**A backend developer who owns the services:** read 000, then 001 and 202. Your service's document is the whole of your integration; the rest of the guide is what happens on the other side of it.

**A full-stack developer working in TypeScript:** read in order through the 100s, then 300, 301 and 302. Skip the 200s until you need to run something that is not the stock host.

**A Rust developer extending the platform:** read 000, 002 and 003 for the contract you are extending, then the 200s in order, then the parts bin.

**A team that owns one part of a larger product:** read 205, then the 100s; the shell is someone else's, and your site runs alone until it is mounted.

**Someone evaluating fsr for a team:** read 000, 001 and 002. They are the argument. If they hold, the rest is detail.

## A note on words

This guide says "body" for a loader or an action, "page" for a route's component and "the report" for what the build and the host print, because those are the words the tools use and this is the layer where the tools are explained. Where the industry has a different name for the same thing, [chapter 000](000-what-fsr-is-made-of.md) gives the translation.
