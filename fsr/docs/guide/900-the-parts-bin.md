# 900. The parts bin

Every block in every crate, one line each, sorted by the itch it scratches. The chapters tell the story; this is the inventory for the day you already know the story and want the name.

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
- **`snapfire_fsr_lower::read_schema`** and **`read_session_defaults`**, the application's own interfaces and the session's defaults, from `schemas/`.

## I want to call a service

- **`snapfire_fsr_service::Services`**, the registry: contract, interceptor chain, one transport per service or a default. `bind` gives a request its handle.
- **`HttpTransport`**, **`GrpcTransport`**, **`LocalTransport`**, **`MockTransport`**, the four transports: over the wire, over the wire in protobuf, in process by function, canned with a recording of every call.
- **`TraceInterceptor`**, **`IdentityInterceptor`**, **`CredentialInterceptor`**, the three that ship; `Interceptor` and `Next` to write another.
- **`Call`** and **`Credentials`**, what an interceptor sees, including the custody a body never does.

## I want to write or run a body

- **`snapfire_fsr_lower::lower_loader`** and **`lower_actions`**, TypeScript in, IR out or a `Residue` with the line.
- **`snapfire_fsr_ir::Expr`**, **`Stmt`**, **`Body`**, the IR itself, JSON-serialisable; **`Builtin`** is the fixed list of pure functions.
- **`snapfire_fsr_ir::Interpreter`**, runs a body against a `RequestCtx`; `evaluate` and `apply` for one expression or one lambda with no request.
- **`IrSource`** and **`IrAction`**, a lowered body as a `DataSource` or an `ActionHandler`.
- **`snapfire_fsr_runtime::DataSource`** and **`ActionHandler`**, the traits a Rust body implements to answer a name.

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

## I want to serve

- **`snapfire_fsr_host::Host`** and **`HostBuilder`**, configuration in, a service over HTTP types out; `route`, `source`, `action`, each with an `_override`, `evaluator`, `shell`, `services_over`, `session_store`.
- **`snapfire_fsr_host::Config`**, **`Deployment`**, **`Located`**, the configuration ladder and what it resolved.
- **`snapfire_fsr_host::actix::serve`**, the shim that mounts the host in actix; `Host::serve` for hyper.
- **`snapfire_fsr::App`** and **`AppBuilder`**, the binding layer under the host, for a host of your own.
- **`snapfire_fsr::Routes`**, plan-file routes plus Rust ones, refusing a pattern claimed twice.
- **`snapfire_fsr_runtime::MatchitMatcher`** and **`TableResolver`**, pattern to entry, entry to plan.

## I want a session or an identity

- **`snapfire_fsr_session::Sessions`**, open before matching, persist at the response; **`SessionConfig`** for the cookie name, TTL and `Secure`.
- **`SessionStore`** and **`MemorySessionStore`**, the store trait and the stock bounded, expiring one.
- **`HmacCodec`**, the signed cookie.
- **`TokenCell`**, custody: the tokens a body cannot reach.
- **`snapfire_fsr_runtime::SessionCell`** and **`Identity`**, what a body sees.
- **`snapfire_fsr_auth::IdentityProvider`**, `begin` and `callback`; **`DevProvider`** for development; **`Auth`** the facade.

## I want the browser to do its part

- **`@snapfire/fsr-client`**: `boot` and `registerIsland` to mount islands, hydrating over server markup; `enableNavigation`, `navigate` and `refresh` for segment-preserving navigation; `action` and `ActionFailure` for calling actions by id; `parsePayload` and `renderSegment` for the wire form.
- **`@snapfire/fsr-client/react`**, the React mounter.
- **`@snapfire/fsr-authoring`**, `Ctx`, `ActionCtx`, `action` and `fail`, the types a body is written against, projected per application into `generated/fsr.ts`.

## I want to test

- **`snapfire_fsr_lower::testing::lower_tests`**, a `*.test.ts` into steps; **`snapfire_fsr_cli::test::run`**, the replay.
- **`MockTransport`** with `returns` and `calls`, for route tests over a host with no backend.
- **`Host::render_to_string`**, **`Host::call_action`** and **`Host::handle`**, a route, an action or a whole request without a socket.

## I want the tools

- **`fsr build`**, **`fsr check`**, **`fsr dev`**, **`fsr serve`**, **`fsr test`**, **`fsr add`**, **`fsr types`**, the commands, all thin fronts over the library.
- **`snapfire_fsr_cli::dev`**, the watch loop; **`serve`**, the stock host over an app with no Rust beside it; **`vendor`**, **`types`** and **`xwpm`**, the dependency side.
- **`snapfire_fsr_lower::ALIASES`**, the five import prefixes, the same table the tsconfigs get.
- **snapfirec**, the TypeScript and CSS compiler, with `paths` for aliases, `--import-map` for externals and `--public-path` for the preload manifest.
