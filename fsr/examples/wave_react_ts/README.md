# wave_react_ts

Google Wave's idea, on FSR: a conversation of blips nested inside the blips they answer, participants you can see arriving, and everyone's typing visible to everyone else before a word of it is kept.

It is the example that needs a WebSocket, and it is the one that shows exactly how much a WebSocket is for. A blip is durable and never touches the socket. A keystroke is not durable and never touches an action.

## Running it

```sh
cargo run -p wave_react_ts
```

Open `http://127.0.0.1:8140/` in two windows, name yourself in each, open the same wave in both and start typing in one.

## One owner of the state

Every wave's state lives in one [plaza](https://crates.io/crates/plaza) `StateController`: the transcript, who is on each wave and what each of them is part way through typing. The controller owns it and its rules are the only writer, running one input at a time on their own task, so nothing in this example holds a lock and there is never a question of which version is real.

Everything that changes a wave is an operation: `Watch`, `Typing`, `Keep`. A view is built per recipient, which is why a reader is never shown a ghost of their own draft: `Views::create_snapshot` is called once for each window and takes the connection it is for.

The transport is the socket the host already terminates. `src/wire.rs` implements plaza's `Session` over it: a connection is an agent, an incoming row is an operation, and the view the controller builds comes back as the two store rows the page already read. The browser never learns that a controller owns any of this.

Reads and writes both go through the controller. `waves.getWave` is a closure plaza runs on the controller's task; `waves.addBlip` submits an operation and then queries, which returns only once that operation has been applied, because the controller does one thing at a time. The action then publishes the wave's topic and every open page revalidates.

## A blip is a document

Every blip stays editable after it is kept, by anyone on the wave. That is the line between this and a chat log: the conversation is the artifact rather than a record of one.

One window holds a blip at a time, and the lock is the state rather than a lock primitive: `Field.edits` holds at most one entry per blip, so the second window to reach for it changes nothing, and the view is what tells both which of them won. While someone holds a blip everyone else watches the words change and cannot take it. Nobody is ever shown their own rewrite, so the textarea a reader is typing in is never written over by the server.

| What | Seam |
| --- | --- |
| Taking a blip, a keystroke in it, letting it go | `Open`, `Rewriting` and `Close` over the socket |
| The rewrite, kept | `waves.editBlip` from an action, then the topic and a revalidation |

An amended blip carries when it was amended and everyone who has amended it, each once and in the order they first came to it, which the transcript shows beside the text.

Which is the same rule as everything else here: durable goes through an action, not durable goes through the socket.

## The two halves

| What | Seam | Why |
| --- | --- | --- |
| A blip, kept for good | a `Keep` operation, then `Host::publish("wave/<id>")` and `live()` | it belongs in the transcript, so the loader is what answers for it |
| Who is here | a `Watch` operation and plaza's presence stream | it is true only while a connection is |
| What someone is typing | a `Typing` operation, one per keystroke | it is superseded by the next keystroke and worth nothing after |

The socket carries rows, `{"key": ..., "value": ...}` up and `{"rows": [...]}` down, and the browser writes each row into the store, so `Presence` and `Under` follow by reading a key. Neither ever fetches.

| Piece | What it is |
| --- | --- |
| `routes/layout.tsx` | the four panes and the only island among them |
| `routes/slots/rail`, `routes/slots/contacts`, `routes/slots/waves` | the navigation, the contacts and the inbox, each a parallel segment |
| `src/field.rs` | the state, the operations, the rules and the view built per recipient |
| `src/wire.rs` | plaza's `Session` over the host's socket |
| `src/backend.rs` | the service the loaders and actions call, over the controller |
| `routes/wave/[id]/page.tsx` | the transcript, rendered on the server |
| `src/ui/Presence.tsx`, `src/ui/Under.tsx`, `src/ui/Body.tsx` | the three islands that read what the controller sends |
| `src/ui/wire.ts` | the one connection the page holds |

## Four panes

The rail, the contacts, the inbox and the open wave are four segments of one route: three parallel slots under `routes/slots/` and the page. Every one of them is rendered on the server.

A rail link is this page under another view, `${path}?view=active`, which is what `ctx.path` is for: a layout and a parallel segment match no parameters of their own, so without it neither the rail could build that link nor the inbox mark the wave that is open. The views themselves are filters the controller applies, `inbox`, `active` for whoever has a connection on a wave and `mine` for the waves this reader has written in, so a view is a query rather than a route.

A click on the rail changes one pane at a time. Every segment carries a digest of what it rendered, so a navigation that only moves `?view=` replaces the rail and the wave list, which read the view, and keeps the contacts pane, the transcript and the composer with whatever is half typed in it. The layout around them is an island and survives too. What this does not do yet is keep a draft inside a pane that genuinely changed, since replacing a region tears down the islands in it.

## Naming yourself is what unlocks writing

A reader with no name may open a wave and read it and nothing else: no composer, no blip offers itself for rewriting, the rules drop every op the window sends and both actions refuse it. Presence leaves it out too, so nobody is listed as `someone`.

The name is a session write, and `me` reaches every composer through the wave's page. Naming asks for the document again rather than revalidating, because a session write changes what every island on the page was rendered from and a fresh document is the honest answer to that.

This example is what found DEFECTS 1.2, now FIXED 10.73: a revalidation used to patch the page and leave the islands under it holding the props they were rendered with, and a blip added by someone else took whichever region a counter landed on, so a new blip could render the header's presence pills and the first blip's text. An island placement owns a named region now, so a reply arriving from another window lands as itself.

## What is server-rendered and what is not

The transcript is. Every blip, in reading order, at the depth `waves.getWave` gave it, rendered in Rust with no JavaScript engine, so the wave is readable before a line of the bundle has run.

Four things are islands, because they cannot be rendered ahead of time: `Presence`, the pill and the participant list; `Under`, the drafts and the composer beneath each blip; `Body`, a blip's text and the rewrite of it; and `Name`. Everything else on the page is lowered. `fsr check app` prints the list and will tell you the moment something falls out of it.

## A wave is not public

`wave/<id>` is followed only by a session that has opened that wave, which the wave's own loader records. The same rule guards the stream and the socket, so neither is a way around the other, and a request for a wave you have not opened is 403.

## What it is worth reading for

**The controller is the only writer.** There is no `Mutex` around wave state anywhere in this example, and no question of ordering: a keystroke, a kept blip and a departure are three operations applied one at a time, and a read taken after a write sees it because both are commands to the same task.

**One connection per page, not per island.** `src/ui/wire.ts` holds the socket and the `live` stream in a module and hands out shares. Islands come and go as the transcript re-renders; the connection does not, and presence does not flicker.

**A store key the build can read.** The rows are `wave/here` and `wave/drafts`, not `wave/<id>/here`. A `useStore` key built from a prop cannot be lowered, so the island falls to the browser and takes the page with it. The key names what the page is showing rather than which wave, since a document shows one.

**Ghosts sit in their own element.** They used to be siblings of the composer, and React reconciles children by position: someone else starting to type shifted the composer down a slot, React remounted it, and the draft in it was lost. Wrapping them in one `<div className="ghosts">` keeps the composer's position fixed, and a draft survives whatever anyone else is doing.

**What the build refuses.** `.slice()`, `[...string]` and `Boolean()` each cost this page its server rendering while they were in it, one at a time, each named with its line by `fsr check`. None of them is an error; each is a component quietly moving to the browser. `Body` cost it once more, for a different reason: a component may hold no statement before its `return`, so the three states of a blip are one ternary rather than three early returns.

## Tests

`cargo test -p wave_react_ts`: that watching a wave puts you on it and leaving takes you off, that a draft is built for everyone but its author and goes when it empties or its author does, that one window holds a blip while the others watch it change, that an amend keeps the rewrite with whoever made it and a departure lets the blip go, that a window with no name reads and writes nothing, that the rules refuse a wave that does not exist and drop a keystroke from a window watching nothing, that a view names which waves the inbox lists and who is a contact, that the service reads and writes through the controller with the read after the write seeing it and the topic going out, and that neither the stream nor the socket is open to a session that has not opened the wave.

`fsr test app`: the depths, which parts are islands, the inbox beside the open wave, the card the request's path marks, the rail's links and presence across every wave.

Checked in two browsers: presence lights up in both, alice's keystrokes appear under the right blip in bob's window before anything is kept, keeping it puts the blip in both transcripts and clears the ghost, and the pill goes dark when the server stops and comes back on its own when it returns. Enter sends from a composer, and `cmd`/`ctrl` with it sends from either, which is the only way to keep a rewrite without reaching for the mouse: the rewrite is a textarea, where Enter is a new line.
