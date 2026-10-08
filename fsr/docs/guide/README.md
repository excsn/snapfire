# The fsr guide

This is the learning layer of fsr's documentation. The example's [README](../../examples/shopping_react_ts/README.md) gets a checkout running in four commands; the [ops console](../../examples/ops_console_react_ts/README.md) is the second example, built to exercise what the storefront never touches. Each crate's README and API reference say exactly what to type. This guide covers the part in between: how fsr handles the parts of a full-stack application, why each piece is shaped the way it is and where you can take a piece over yourself.

Every chapter answers one question, reads in one sitting and names a lab: something to do to the running example that makes the chapter's claim observable and usually lets you watch it fail too. Do the labs, since they show each claim working on the example.

## Who each chapter is for

fsr has two kinds of developer and the guide says which one it is talking to at the top of every chapter.

- **App developers** write TypeScript under `app/`: routes, loaders, actions, pages, components, schemas and tests. They never write Rust and never run Node.
- **Platform developers** write Rust against the host and the crates: overriding a body, adding a service transport, mounting the host somewhere else, extending the platform for a team.

A chapter marked **everyone** is one both need. Nothing in the app developer chapters requires reading the platform ones; the reverse is almost true: a platform developer should read 100 and 101 once, since the bodies they override are written there.

## The chapters

**Foundations:**

- [000. What fsr is made of](000-what-fsr-is-made-of.md), TypeScript as the application language, Rust as the runtime, the two artifacts that are the truth, what fsr refuses to do and the vocabulary map. Everyone.
- [001. Calling a service without a client](001-one-contract-no-client-code.md), how a service's own document becomes a typed call nobody wrote. Everyone.
- [002. How loaders and actions are lowered](002-a-body-is-data.md), why a loader is lowered rather than run, what residue is and why the report always says where a body runs. Everyone.
- [003. Server rendering without a JavaScript engine](003-rendered-where-it-is-cheapest.md), how a React page is rendered on the server with no JavaScript engine, which of its components become islands, what hydrates over them and what the browser reads instead of computing again. Everyone.

**The application:**

- [100. Routes, loaders and pages](100-routes-loaders-and-pages.md), the file conventions, params and query, the props a page receives and navigation that keeps the layout. App developers.
- [101. Actions and the session](101-actions-and-the-session.md), schemas, session defaults, guards and the cart as the worked case. App developers.
- [102. Components the server renders](102-components-the-server-renders.md), what a component may say, helpers and the standard library, `useState`, the store in place of signals, what stays in the browser, what the server computes for it and an island the server drives. App developers.
- [103. Testing a body and a page](103-testing-a-body.md), `describe`, `expect` and mock functions over a replayed body, the contract checks, the trace, page tests over a DOM with hydration, queries by role, a user at the keyboard, loading a route and clicking through it, plus `fsr test`. App developers.
- [104. Islands in another framework](104-islands-in-another-framework.md), a `.vue` file placed by a template, the plugin that compiles it with no Node, scoped styles reaching the head, why a template with no state loads no framework and the application with no React in it. App developers.
- [105. Applications without a framework](105-no-framework-at-all.md), custom elements a template writes and the browser upgrades, a shadow root the server renders, the store read with no adapter, htmx regions over fragments the host renders, the two events that let a library and the navigator share a document and a page written as a Tera template the host renders from the file. App developers.
- [106. Two frameworks on one page](106-two-frameworks-on-one-page.md), how islands are dispatched by module id, one store under two adapters, one router over segments from different frameworks, htmx beside them and what each one costs the page. App developers, plus anyone weighing a migration.
- [107. Moving tests from Jest or Vitest](107-moving-tests-from-jest-or-vitest.md), what carries over from a Jest, Vitest or Testing Library suite, what changes and a spec moved end to end. App developers moving a suite.
- [108. Images and fonts](108-images-and-fonts.md), an image as an import the build resizes into a `<picture>`, the catalog whose photos are a record indexed by data, a remote image through a named source, a font under `fonts/` served with a fallback sized so nothing moves and the one table that moves every asset directory. App developers.

**The host:**

- [200. The stock host and its configuration](200-the-stock-host-and-its-configuration.md), the config ladder, what the host infers and the boot report. Platform developers.
- [201. Graduating to Rust](201-graduating-to-rust.md), taking one name back from the plan file and the rule that makes each override explicit. Platform developers.
- [202. Services and transports](202-services-and-transports.md), HTTP, gRPC, interceptors and why application code never sees a token. Platform developers.
- [203. Sessions and identity](203-sessions-and-identity.md), the signed cookie, the store, who the request is and where a login goes. Platform developers.
- [204. Reloading in place](204-reloading-in-place.md), the tables a request reads, what a reload swaps and what it refuses and why `fsr dev` reloads in place instead of restarting. Platform developers.
- [205. Sites](205-sites.md), a team's application built as a site, the shell that mounts it under a path, what passes between shell and site and how a deploy switches a site to a new version. Everyone.
- [206. What a request did](206-what-a-request-did.md), the trace behind a slow or wrong page: the seven spans the framework opens, reading them with no collector wired, what they cost when nobody is watching and how to get them out. Everyone.

**Tooling:**

- [300. The build and the dev loop](300-the-build-and-the-dev-loop.md), `fsr build`, `fsr check`, `fsr dev`, `fsr serve` and why `generated/` is not committed. Everyone.
- [301. Dependencies without npm](301-dependencies-without-npm.md), `fsr add`, `fsr types`, the import map and what xwpm changes. Everyone.
- [302. Imports and aliases](302-imports-and-aliases.md), the five prefixes and where each of the three readers resolves them. Everyone.
- [303. The deploy tree](303-the-deploy-tree.md), `fsr bundle`, why `serve/` is the only servable directory and how the routes a server answers stay the ones the host serves. Everyone.
- [304. Checking a deployment](304-checking-a-deployment.md), `fsr doctor`, the six checks it runs and why each one is a thing the host will not refuse to start over. Everyone.
- [305. Third-party scripts](305-third-party-scripts.md), the fixed head rows as layout meta, a library as a module the entry imports, a consent banner that loads a vendor from its callback and the deployment's own values under `[public]`. App developers.

And one appendix:

- [900. The parts bin](900-the-parts-bin.md), every block in every crate, one line each, grouped by what you are trying to do.

## Reading paths

**A frontend developer who knows Next or Remix:** read 000 for the vocabulary, then 100, 101, 102, 103. Chapter 003 will read like the rendering model you already have, minus the engine. Start the project with `fsr new <dir> --with react`: a plain `fsr new` is a bare application with no framework. 102 is written for React.

**A frontend developer who writes Vue:** read 000, then 100 and 101 for the templates and bodies, then 104. Your components are islands and the pages around them are templates. 102 covers the same split for React and 106 covers what it costs to run both at once.

**A frontend developer who would rather have no framework:** read 000, then 100 and 101, then 105. Your pages are templates, your interactive pieces are elements the browser defines and your regions are fragments; 003 says why the server can render all of it. The end of 105 is for the developer who would rather write those pages in Tera than in TSX.

**A backend developer who owns the services:** read 000, then 001 and 202. Your service's OpenAPI or proto document is all fsr needs from you and the rest of the guide covers what the application does with it.

**A full-stack developer working in TypeScript:** read in order through the 100s, then 300, 301, 302, 303 and 304. Skip the 200s until you need to run something that is not the stock host.

**A Rust developer extending the platform:** read 000, 002 and 003 for the contract you are extending, then the 200s in order, then the parts bin.

**A team that owns one part of a larger product:** read 205, then the 100s; the shell is someone else's and your site runs alone until it is mounted.

**Someone evaluating fsr for a team:** read 000, 001 and 002. Those three cover the claims the rest of the guide depends on.

## A note on words

This guide says "body" for a loader or an action, "page" for a route's component and "the report" for what the build and the host print, because those are the words the tools use and this is the layer where the tools are explained. Where the industry has a different name for the same thing, [chapter 000](000-what-fsr-is-made-of.md) gives the translation.
