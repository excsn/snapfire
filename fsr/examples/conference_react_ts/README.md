# conference_react_ts

A one-day conference programme: the day's talks, a page per talk, a schedule you keep and two panels beside them of which one is always down.

What it shows is what is absent. There is no `Cargo.toml`, no `src/*.rs`, no `build.rs` and no binary of its own. Every route, loader, action and component is TypeScript, the build turns them into `generated/plan.sexp` and the stock host reads that plan at boot. The service behind the whole application is an OpenAPI document with a file of canned answers beside it.

## Running it

```sh
fsr dev app
```

That is all of it: no `cargo run`, no `npm install`. `fsr serve app` serves what is already built and `fsr test app` runs the suite.

The programme is on <http://127.0.0.1:8150/>.

## How it is put together

| Piece | What it is |
| --- | --- |
| `clients/program.openapi.json` | the one service, four methods, two of them carrying a cache policy |
| `clients/program.mock.json` | what those four methods answer, `listSponsors` with a failure |
| `schemas/session.ts` | the session's shape and its defaults, which is what makes `session.saved` typed |
| `schemas/program.ts` | the action's input type, named in the contract the build emits |
| `routes/layout.tsx` | the masthead, the nav and the two panels, over `layout.loader.ts`, declared `tree` so the page renders in its React root |
| `routes/page.tsx` | the day, filtered by `?track=` |
| `routes/talk/layout.tsx` | a second layout between the masthead and the talk, a tree too, so a click to another talk renders it under the same crumbs |
| `routes/talk/[id]/` | the talk, its actions and the boundary that catches an id off the programme |
| `routes/saved/` | the session read back as a page, cached by nothing |
| `routes/slots/announcements/` | a parallel segment with its own loader and fallback |
| `routes/slots/sponsors/` | the same, behind a service that fails |
| `src/ui/Saved.tsx` | the masthead island: store-backed count, React state for the panel |
| `src/ui/SaveTalk.tsx` | the action call, optimistic against the store |
| `src/ui/Feedback.tsx` | placed with `island(Feedback, { when: "visible" })` |

## Everything is lowered

`fsr build app` prints what answers each name and every row says `lowered`: no body runs in an engine and nothing is bound in Rust, because there is no Rust here to bind it in.

Read `app/generated/plan.sexp` to see what the server will actually do. The day's loader comes out as:

```
(source $root
  lowered
  (module routes/page.loader.ts)
  (body
    (let talks (call program listTalks))
    (let conference (call program getConference))
    (let track (?? (query track) "all"))
    (let shown (if (== track "all") talks (filter talks (fn (t) (== (. t track) track)))))
    ...
```

Two `let`s in a row, neither reading the other, so the interpreter issues both calls together. Nothing in the TypeScript arranged that; it is a property of the block.

The same file shows the session defaults folded in. `session.saved` is written plainly in the loader and reads `(?? (session saved) (obj))` in the plan, because `schemas/session.ts` declares `saved: {}` beside the type.

## What each thing proves

| Capability | Where |
| --- | --- |
| Nested layout plus dynamic segment | `routes/layout.tsx` over `routes/talk/layout.tsx` over `/talk/{id}` |
| Two loaders resolving in parallel | the two independent calls in `routes/page.loader.ts`, plus the two slots beside the page |
| An action mutating and revalidating | `save` guards against the programme, writes the session and the count in the masthead moves |
| One island on load, one on visible | `<Island when="load">` around the save control, `island(..., { when: "visible" })` around the pace control |
| A segment whose service call fails | `listSponsors` answers `$fail`, the panel falls to `slots/sponsors/error.tsx` and the page is otherwise whole |
| A cached segment plus an uncached one | `listTalks` and `getConference` carry `x-sf-cache`, `listAnnouncements` and `listSponsors` do not |
| Metadata from loader data | `export const meta` in the talk loader and the saved loader |
| Client navigation preserving layout state | open the masthead panel, click a talk: the page region is replaced and the panel stays open |
| One React tree for a layout and its page | both layouts are `tree(...)`: the talk hydrates in the talk layout's root with its props script consumed and a click to another talk renders the new page from its props inside the live layout; `tests/tree.spec.tsx` |

The island timings are the one thing the test suite cannot tell apart, since the spec harness reports every observed element as in view. Scroll the talk page in a browser instead: the pace control hydrates when it comes into view and not before.

## A service with no backend

`transport = "mock"` in `config/app.toml` is the whole backend story. The host reads `clients/program.mock.json` at boot and answers each method from it, so the application runs against its contract with nothing else running.

That constrains the data: a mock is keyed by method and never by arguments, so `listTalks` answers the same list whatever it is asked. The talk page picks its talk out of that list rather than calling `getTalk(id)`, which is why its loader filters and guards:

```ts
const matches = talks.filter((t) => t.id === params.id);
if (matches.length === 0) fail("not_found", "there is no talk with that id");
const talk = matches[0];
```

A failure is declared the same way. `listSponsors` answers `{"$fail": {"kind": "unavailable", "message": "..."}}`, so the panel is down on every request and the degradation is something you can look at rather than something you have to break the machine to see.

## Deploying it

```sh
fsr bundle app --out dist
```

77 files, no `.tsx` among them. The thing that goes beside the tree is `fsr` itself: this application has no binary of its own, so the stock host is the binary.

```sh
fsr serve dist/app
```

## The lab

Build it and read the report: `fsr build app` names every source, action and component; each one says `lowered`.

Read the cache policy where it lives. `app/generated/contracts/program.json` carries `getConference` and `listTalks` with their ttl and their `program` tag while the other two carry nothing, because the policy is a property of the method rather than of a call site. Deleting `[cache.data]` from `config/app.toml` turns every one of them off without touching a loader.

Watch the cache work. Load `/` twice, then fetch `/__fsr/traces` and compare the two requests: the `call` span for `getConference` says `cache: miss` the first time and `cache: hit` the second, while `listAnnouncements` says `cache: none` both times because its method carries no policy.

Break the contract. Rename `listTalks` in `clients/program.mock.json` and restart: the host still boots, because a mock answers what it holds, while the day's page fails on the first request with the method it could not find. Rename it in `clients/program.openapi.json` instead and the build refuses, because the loader names a method the contract does not carry.

Add a route. Make `app/routes/rooms/page.tsx` returning a list, run `fsr build app`, then find it in the plan and in the report with no file edited anywhere else.
