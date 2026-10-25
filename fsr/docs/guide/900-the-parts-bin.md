# 900. The parts bin

Every block in every crate, one line each, sorted by what you would be looking for. The chapters explain how the pieces fit; this is the index for when you already know that and want a name.

**For:** everyone, platform developers most.

## I want to know what the application is

- **`snapfire_fsr_plan::Manifest`**, the plan file: routes with their plan nodes, source rows, action rows and component rows, with the owner of each. Read at boot, written by the build.
- **`snapfire_fsr_service::Contract`**, the merged description of every service and type, with `check_call` and `check_value` for arguments and responses.
- **`snapfire_fsr_cli::build`** and **`write`**, the build as a library, which is what a `build.rs` calls.
- **`snapfire_fsr_cli::Report`** and **`snapfire_fsr::Report`**, the table the build and the host print.
- **`snapfire_fsr_plan::Manifest::namespaced`** and **`snapfire_fsr_service::Contract::namespaced`**, the plan file and the contract as a site's, every id prefixed with its name.
- **`snapfire_fsr_cli::ShellContract`**, `generated/shell.json`, what a site is built against.
- **`snapfire_fsr_host::Mount`** and **`HostBuilder::mount`**, a site's artifact mounted under its prefix; **`Host::reload`**, the tables swapped in place.
- **`snapfire_fsr_sites::mount_all`**, **`resolve`**, **`hash_dir`** and **`watch`**, the `[sites]` table resolved, hashed, mounted and reread.

## I want to describe a service

- **`snapfire_fsr_service::import`**, an OpenAPI document into a contract plus the HTTP routes each method needs.
- **`snapfire_fsr_service::import_proto`**, a `.proto` into a contract plus the descriptors and method paths a gRPC call needs, compiled without protoc.
- **`snapfire_fsr_service::import_graphql`**, a GraphQL schema and its named operations into a contract, each operation a method, plus the operation text `GraphqlTransport` posts.
- **`snapfire_fsr_lower::read_schema`** and **`read_session_defaults`**, the application's own interfaces and the session's defaults, from `schemas/`.

## I want to call a service

- **`snapfire_fsr_service::Services`**, the registry: contract, interceptor chain, one transport per service or a default. `bind` gives a request its handle.
- **`HttpTransport`**, **`GrpcTransport`**, **`LocalTransport`**, **`MockTransport`**, the four transports: over the wire, over the wire in protobuf, in process by function, canned with a recording of every call.
- **`TraceInterceptor`**, **`IdentityInterceptor`**, **`CredentialInterceptor`**, the three that ship; `Interceptor` and `Next` to write another. `CredentialInterceptor::refreshing` with a **`Refresher`** renews an expired token before a call carries it.
- **`Call`** and **`Credentials`**, what an interceptor sees, including the custody a body never does.

## I want to write or run a body

- **`snapfire_fsr_lower::lower_loader`** and **`lower_actions`**, TypeScript in, IR out or a `Residue` with the line.
- **`snapfire_fsr_ir::Expr`**, **`Stmt`**, **`Body`**, the IR itself, JSON-serialisable; **`Builtin`** is the fixed list of pure functions.
- **`snapfire_fsr_ir::Interpreter`**, runs a body against a `RequestCtx`; `evaluate` and `apply` for one expression or one lambda with no request.
- **`IrSource`** and **`IrAction`**, a lowered body as a `DataSource` or an `ActionHandler`.
- **`snapfire_fsr_runtime::DataSource`** and **`ActionHandler`**, the traits a Rust body implements to answer a name.
- **`snapfire_fsr_core::ext`**, what the Rust half of a native pair is written against: `Reach`, `Ambient`, `Fail` and the helpers that read its arguments. `HostBuilder::extension` registers one; the host re-exports the module.

## I want to render

- **`snapfire_fsr_core::Node`**, the payload tree: text, raw markup, a sequence, a client island with optional server markup, a pending slot.
- **`snapfire_fsr_core::PlanNode`**, one route's plan: module, data source, children by slot, deferral, fallback and error modules, cache key.
- **`snapfire_fsr_runtime::Evaluator`**, module and props in, chunks of nodes out; **`NullEvaluator`** emits an island with no markup; **`Evaluators`** dispatches by module.
- **`snapfire_fsr_ir::IrEvaluator`**, renders lowered components in Rust; **`Tmpl`** and **`Component`** are what it renders.
- **`snapfire_fsr_lower::component::ComponentSet`**, lowers a page and everything it renders.
- **`snapfire_fsr_tera`**, a Tera instance with `island`, `slot` and `head` functions, so a template is an evaluator.
- **`snapfire_fsr_host::shell::DocumentShell`**, the stock document: head, import map, stylesheets, entry script.
- **`snapfire_fsr_runtime::Runtime`** and **`assemble`**, the assembler: match, resolve, load, evaluate, stitch, with **`NodeCache`** for subtrees, **`MemoryCache`**, **`FibreCache`** and **`NoCache`** as implementations and **`SegmentKeyer`** for navigation identity.
- **`snapfire_fsr_payload::html_serialize`** and the row encoding, the tree as HTML and as the wire form the client reads.
- **`snapfire_fsr_runtime::html_stream`** and **`wire_stream`**, the same with deferred slots streamed in.
- **`snapfire_tailwind::Compiler`**, Tailwind CSS v4 from a stylesheet's own configuration: `build` for the classes given, `candidate_css` for one of them.
- **`snapfire_fsr_runtime::DocumentTheme`**, a request's theme and scheme, written on `<html>` and in the payload's eager head row.

## I want to serve

- **`snapfire_fsr_host::Host`** and **`HostBuilder`**, configuration in, a service over HTTP types out; `route`, `source`, `action`, each with an `_override`, `evaluator`, `shell`, `services_over`, `session_store` and, with the `auth` feature, `identity`. Sign-in is the host's `auth` feature, with `oidc` and `passkey` for those methods; `fsr` carries all three.
- **`snapfire_fsr_host::Loader`**, **`Artifact`**, **`Config`**, **`Deployment`**, **`Located`**, how an artifact is read, what was read, the configuration ladder and what it resolved; `Loader::secrets` for how a `.c5encval` decrypts.
- **`snapfire_fsr_host::actix::serve`**, the shim that mounts the host in actix; `Host::serve` for hyper.
- **`snapfire_fsr::App`** and **`AppBuilder`**, the binding layer under the host, for a host of your own.
- **`snapfire_fsr::Routes`**, plan-file routes plus Rust ones, refusing a pattern claimed twice.
- **`snapfire_fsr_runtime::MatchitMatcher`** and **`TableResolver`**, pattern to entry, entry to plan.

## I want a session or an identity

- **`snapfire_fsr_session::Sessions`**, open before matching, persist at the response; **`SessionConfig`** for the cookie name, TTL and `Secure`.
- **`SessionStore`** and **`MemorySessionStore`**, the store trait and the stock bounded, expiring one; **`SessionRecord::encode`** and `decode` for a store of your own.
- **`HmacCodec`**, the signed cookie; **`SealedCookie`** and `Sessions::sealed`, the whole record sealed into the cookie with no store.
- **`TokenCell`**, custody: the tokens a body cannot reach.
- **`snapfire_fsr_runtime::SessionCell`** and **`Identity`**, what a body sees.
- **`snapfire_fsr_auth::IdentityProvider`**, `begin`, `advance` and `refresh`: a sign-in as steps that redirect, present data to the page or complete, as **`Next`**; **`Start`** says what a flow begins from. **`Auth`** is the facade and **`Step`** what it answers the adapter.
- **`Methods`**, several providers on one login page by name; **`Sequence`**, a second factor after another provider.
- **`DevProvider`**, a user table for development; its `stand_in` signs in with no passwords.
- **`snapfire_fsr_oidc::OidcProvider`** over an **`OidcConfig`**, OpenID Connect against any issuer, with Google, Microsoft and Apple presets.
- **`snapfire_fsr_passkey::Passkeys`** over a **`PasskeyStore`**, WebAuthn sign-in and enrolment; **`FilePasskeys`** and **`MemoryPasskeys`** keep them.
- **`snapfire_fsr_host::ServiceProvider`**, any auth service as a provider over a client's `beginSignIn` and `advanceSignIn`.

## I want the browser to do its part

- **`@snapfire/fsr/client`**: `signIn`, `enrol` and `answer` run a sign-in from the page, the passkey prompt included; `boot` and `registerIsland` to mount islands, hydrating over server markup; `discard` to end the islands in a subtree before it is removed; `enableNavigation`, `navigate` and `refresh` for segment-preserving navigation; `action` and `ActionFailure` for calling actions by id; `parsePayload` and `renderSegment` for the wire form.
- **`@snapfire/fsr/client/react`**, the React mounter, patcher and unmounter.
- **`@snapfire/fsr/host`**, `Ctx`, `ActionCtx`, `action` and `fail`, the types a body is written against, projected per application into `generated/host.ts` beside every host standard module. **`@snapfire/fsr/client`**, the browser runtime and the standard modules a server render and the browser both run.

## I want to test

- **`snapfire_fsr_lower::testing::lower_tests`**, a `*.test.ts` into steps; **`snapfire_fsr_cli::test::run`**, the replay.
- **`MockTransport`** with `returns` and `calls`, for route tests over a host with no backend.
- **`Host::render_to_string`**, **`Host::call_action`** and **`Host::handle`**, a route, an action or a whole request without a socket.

## I want the tools

- **`fsr build`**, **`fsr check`**, **`fsr dev`**, **`fsr serve`**, **`fsr test`**, **`fsr add`**, **`fsr types`**, the commands, all thin fronts over the library.
- **`snapfire_fsr_cli::dev`**, the watch loop; **`serve`**, the stock host over an app with no Rust beside it; **`vendor`**, **`types`** and **`xwpm`**, the dependency side.
- **`snapfire_fsr_lower::ALIASES`**, the five import prefixes, the same table the tsconfigs get.
- **snapfirec**, the TypeScript and CSS compiler, with `paths` for aliases, `--import-map` for externals, `--public-path` for the preload manifest and `--tailwind-classes` with `--tailwind-themes` for a stylesheet that imports Tailwind.
- **`snapfire_fsr_cli::tailwind`**, the classes the plan renders, the `tailwind/unlisted-class` findings and the card table.
