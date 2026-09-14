# wave_react_ts

Google Wave's idea, on FSR: a conversation of blips nested inside the blips they answer, participants you can see arriving and everyone's typing visible to everyone else before a word of it is kept.

It is the example that needs a WebSocket and it is the one that shows exactly how much a WebSocket is for. A blip is durable and never touches the socket. A keystroke is not durable and never touches an action.

## Running it

```sh
cargo run -p wave_react_ts
```

Open `http://127.0.0.1:8140/` in two windows, name yourself in each, open the same wave in both and start typing in one.

## One owner of the state

Every wave's state lives in one [plaza](https://crates.io/crates/plaza) `StateController`: the transcript, who is on each wave and what each of them is part way through typing. The controller owns it and its rules are the only writer, running one input at a time on their own task, so nothing in this example holds a lock and there is never a question of which version is real.

Everything that changes a wave is an operation: `Watch`, `Typing`, `Keep`. A view is built per recipient, which is why a reader is never shown a ghost of their own draft: `Views::create_snapshot` is called once for each window and takes the connection it is for.

The transport is the socket the host already terminates. `src/wire.rs` implements plaza's `Session` over it: a connection is an agent, an incoming row is an operation and the view the controller builds comes back as the two store rows the page already read. The browser never learns that a controller owns any of this.

Reads and writes both go through the controller. `waves.getWave` is a closure plaza runs on the controller's task; `waves.addBlip` submits an operation and then queries, which returns only once that operation has been applied, because the controller does one thing at a time. The action then publishes the wave's topic and every open page revalidates.

## Blips stay editable

Every blip stays editable after it is kept, by anyone on the wave. That is the difference from a chat log, where a message is fixed once it is sent.

A blip is its blocks, one per top-level markdown block. Each block has an id that stays with it while its text changes. One window holds a block at a time and the lock is the state rather than a lock primitive: `Field.edits` holds at most one entry per blip and block, so the second window to reach for it changes nothing and the view is what tells both which of them won. Two windows can rewrite two blocks of one blip at once. Rewriting the whole blip as its markdown source holds every block of it and waits while anyone holds one. While someone holds a block everyone else watches the words change in its place and cannot take it. Nobody is ever shown their own rewrite, so the textarea a reader is typing in is never written over by the server.

A reply anchors to a block by its id. A reply to a list item anchors to the block's id and the item's place in it (`b3.1`), so writing a paragraph above a block leaves its replies where they were. A block rewritten as several blocks keeps its id on the first; one rewritten as nothing is gone and its replies join the blip's own. A save of the whole source is diffed into the blocks, so a block the new text kept or edited keeps its id.

| What | Seam |
| --- | --- |
| Taking a block or the whole blip, a keystroke in it, letting it go | `Open`, `Rewriting` and `Close` over the socket, each naming the block (empty for the whole blip) |
| The rewrite, kept | `waves.editBlip` from an action, then the topic and a revalidation |

An amended blip carries when it was amended and everyone who has amended it, each once and in the order they first came to it, which the transcript shows beside the text.

Which is the same rule as everything else here: durable goes through an action, not durable goes through the socket.

## The two halves

| What | Seam | Why |
| --- | --- | --- |
| A blip, kept for good | a `Keep` operation, then `Host::publish("wave/<id>")` and `live()` | it belongs in the transcript, so the loader is what answers for it |
| Who is here | a `Watch` operation and plaza's presence stream | it is true only while a connection is |
| What someone is typing | a `Typing` operation, one per keystroke | it is superseded by the next keystroke and worth nothing after |

The socket carries rows, `{"key": ..., "value": ...}` up and `{"rows": [...]}` down and the browser writes each row into the store, so `Presence` and `Under` follow by reading a key. Neither ever fetches.

| Piece | What it is |
| --- | --- |
| `routes/layout.tsx` | the four panes and the only island among them |
| `routes/slots/rail`, `routes/slots/contacts`, `routes/slots/waves` | the navigation, the contacts and the inbox, each a parallel segment |
| `src/field.rs` | the state, the operations, the rules and the view built per recipient |
| `src/wire.rs` | plaza's `Session` over the host's socket |
| `src/backend.rs` | the service the loaders and actions call, over the controller |
| `src/blocks.rs` | a body split into its blocks and a rewrite's blocks given their ids |
| `src/gadgets.rs` | what a gadget block is, its state and the rules of a move and a vote |
| `routes/wave/[id]/page.tsx` | the transcript, rendered on the server |
| `src/ui/Presence.tsx`, `src/ui/Under.tsx`, `src/ui/Body.tsx`, `src/ui/Block.tsx` | the four islands that read what the controller sends |
| `src/ui/Playback.tsx` | the scrubber over the wave's log |
| `src/ui/Gadget.tsx` | a gadget in a blip, a server-mode island that loads no module |
| `routes/actions.ts` | `name`, `blip`, `amend`, `play`, `reset` and `vote`, the six durable writes |
| `src/ui/wire.ts` | the one connection the page holds |

## Four panes

The rail, the contacts, the inbox and the open wave are four segments of one route: three parallel slots under `routes/slots/` and the page. Every one of them is rendered on the server. There is no header: the wordmark and the reader's name sit at the foot of the side pane, and the name opens the reader's settings, where it can be changed.

Where the panes sit side by side the app is the window's height and each pane scrolls on its own. The open wave's transcript scrolls between its scrubber and its composer, so both stay in view. Where the panes stack the page scrolls as a whole and the scrubber and the composer stick to the window's edges.

A rail link is this page under another view, `${path}?view=active`, which is what `ctx.path` is for: a layout and a parallel segment match no parameters of their own, so without it neither the rail could build that link nor the inbox mark the wave that is open. The views themselves are filters the controller applies, `inbox`, `active` for whoever has a connection on a wave and `mine` for the waves this reader has written in, so a view is a query rather than a route.

A click on the rail changes one pane at a time. Every segment carries a digest of what it rendered, so a navigation that only moves `?view=` renders the rail and the wave list again, the two that read the view. It keeps the contacts pane, the transcript and the composer with whatever is half typed in it. The layout around them is an island and survives too. Since only the query moved, the two panes that did change are morphed rather than replaced, so an island inside either keeps what it holds.

## Naming yourself

A reader with no name is asked for one in a bar across the top of the page, which goes once they give it. Until then they may open a wave and read it and nothing else: no composer, no blip offers itself for rewriting, the rules drop every op the window sends and both actions refuse it. Presence leaves it out too, so nobody is listed as `someone`.

The name is a session write and `me` reaches every composer through the wave's page. Naming asks for the document again rather than revalidating, because a session write changes what every island on the page was rendered from and a fresh document is the honest answer to that.

This example is what found DEFECTS 1.2, now FIXED 10.73: a revalidation used to patch the page and leave the islands under it holding the props they were rendered with and a blip added by someone else took whichever region a counter landed on, so a new blip could render the header's presence pills and the first blip's text. An island placement owns a named region now, so a reply arriving from another window lands as itself.

## What is server-rendered and what is not

The transcript is. Every blip, in reading order, at the depth `waves.getWave` gave it, rendered in Rust with no JavaScript engine, so the wave is readable before a line of the bundle has run.

Six things are browser islands, because they cannot be rendered ahead of time: `Presence`, the pill and the participant list; `Under`, the drafts and the composer beneath each blip; `Body`, a blip's footer and the rewrite of all of it; `Block`, one block and the rewrite of it; `Playback`, the scrubber; and `Name`. `Gadget` is the seventh island and the odd one: it is in server mode, so no module is loaded for it. A `Block` is placed with its block's markup as children, which the server renders into a region the island adopts. A revalidation that changes them morphs that region in place, so a composer open under a list item inside it keeps what was typed. Everything else on the page is lowered. `fsr check app` prints the list and will tell you the moment something falls out of it.

## Gadgets, in server mode

A gadget is state everyone on a wave shares inside a blip. It is written into the blip's markdown as a fenced block whose info string names it:

````md
```gadget noughts
```

```gadget yesno
Keep the clock in the corner?
```

```gadget poll
When do we launch?
Thursday
Friday
Next week
```
````

`noughts` is a board of noughts and crosses. `yesno` asks the fence's first line and takes one answer each of Yes, No or Maybe. `poll` asks the first line and offers every line after it as a choice. A fence of any other kind is code. The composer's Gadget menu writes the fence for you, with what was typed so far as the question.

The fence is a block like any other, so it has an id. The blip keeps the gadget's state under that id (`Blip.gadgets`). Writing above or below it leaves the state where it was. Rewriting the fence as another kind starts the new one from nothing. Every move and every answer is a change in the wave's log, so playback shows the board filling and the answers arriving.

`Gadget` is a server-mode island placed where the block is. The browser fetches no module for it. A click posts the element and the handler to the host, which runs the lowered handler. The whole body of a cell's handler is one call:

```tsx
onClick={(e) => void actions.$root.play({ wave, blip, block, cell: Number((e.target as HTMLButtonElement).value) })}
```

`actions.$root.play` is the action exported as `play` from `routes/actions.ts`. The generated client nests a callable under the route's directories. `routes/` itself has no directory to nest under, so `$root` stands in for it. The `$` is an ordinary property name.

Everything a click means lives in `src/gadgets.rs` behind the action: whose turn it is, whether the cell is free, whether that finished the game, whether an answer is one the vote offers. The handler could not do that work itself, because a server-mode handler may not contain an `if` and deciding a move is all conditionals. Naming an action is the way out: an action is Rust and has no restrictions.

Two rules of the seam apply here.

- The cell number or the answer rides on the element as `value` and comes back through `e.target`. A handler may not read a name the markup bound, because the host re-runs the component's own bindings and has no loop variable to give it. The build refuses that placement by name. `wave`, `blip` and `block` are the island's own props, so the handler reads them freely.
- The action publishes the wave's topic like every other write, so a move made in one window reaches the others without a reload.

## Playback

Every durable change to a wave is logged: a kept blip, an amend, a move on a board, a fresh board and an answer to a vote. `Wave::apply` in `src/field.rs` is the only place one lands, so the wave is its log applied to an empty wave and `Wave::replayed(n)` is the wave after `n` changes. The seed is written as its log too, so playback starts from nothing. Keystrokes, holds and a move the rules refuse are not logged. The log is held in memory with the rest of the field.

A step is the wave's own page under `?at=`. The loader passes it to `waves.getWave`, which replays the log that far and lights the blip or the gadget the step changed. The page under `?at=` places no composer, no editor and no reply, so nobody writes on the past. No gadget can be used either.

`src/ui/Playback.tsx` is the scrubber. Moving it is a navigation that takes the place of the current history entry and leaves the window where it was:

```ts
void navigate(`${url.pathname}${url.search}`, true, { replace: true, scroll: false });
```

Only the query changes, so the navigator morphs the page rather than replacing it and the scrubber keeps its DOM and its state: a drag carries on and play keeps playing. Whenever the scrubber moves, by a drag, a step button or play, the blip or gadget that step changed is scrolled smoothly into the middle of the transcript when it is out of view, the last change included once the wave is live again. The scrubber stays in view while it does. The end of the log is the wave as it stands.

## A wave is not public

`wave/<id>` is followed only by a session that has opened that wave, which the wave's own loader records. The same rule guards the stream and the socket, so neither is a way around the other and a request for a wave you have not opened is 403.

## What it is worth reading for

**The controller is the only writer.** There is no `Mutex` around wave state anywhere in this example and no question of ordering: a keystroke, a kept blip and a departure are three operations applied one at a time and a read taken after a write sees it because both are commands to the same task.

**One connection per page, not per island.** `src/ui/wire.ts` holds the socket and the `live` stream in a module and hands out shares. Islands come and go as the transcript re-renders; the connection does not and presence does not flicker.

**A store key the build can read.** The rows are `wave/here` and `wave/drafts`, not `wave/<id>/here`. A `useStore` key built from a prop cannot be lowered, so the island falls to the browser and takes the page with it. The key names what the page is showing rather than which wave, since a document shows one.

**Ghosts sit in their own element.** They used to be siblings of the composer and React reconciles children by position: someone else starting to type shifted the composer down a slot, React remounted it and the draft in it was lost. Wrapping them in one `<div className="ghosts">` keeps the composer's position fixed and a draft survives whatever anyone else is doing.

**What the build refuses.** `.slice()`, `[...string]` and `Boolean()` each cost this page its server rendering while they were in it, one at a time, each named with its line by `fsr check`. None of them is an error; each is a component quietly moving to the browser. `Body` cost it once more, for a different reason: a component may hold no statement before its `return`, so the three states of a blip are one ternary rather than three early returns.

## Tests

`cargo test -p wave_react_ts`: that watching a wave puts you on it and leaving takes you off, that a draft is built for everyone but its author and goes when it empties or its author does, that one window holds a blip while the others watch it change, that an amend keeps the rewrite with whoever made it and a departure lets the blip go, that a window with no name reads and writes nothing, that the rules refuse a wave that does not exist and drop a keystroke from a window watching nothing, that a view names which waves the inbox lists and who is a contact, that the service reads and writes through the controller with the read after the write seeing it and the topic going out that neither the stream nor the socket is open to a session that has not opened the wave, that two windows hold two blocks of one blip while the whole blip waits for both, that a block rewrite splits or removes that block alone, that a rewrite of the whole blip keeps the ids of the blocks it kept or edited, that a reply stays with its block when a block is written above it, that the log holds every durable change and replaying it rebuilds the wave, that the service shows the wave after any step of its log and that a gadget keeps its state while the text around it changes.

`fsr test app`: the depths, which parts are islands, the inbox beside the open wave, the card the request's path marks, the rail's links, presence across every wave, a step of playback with nothing on it to write with, the scrubber surviving its own navigation, a vote answered where its fence is and the Gadget menu writing the fence.

Checked in two browsers: presence lights up in both, alice's keystrokes appear under the right blip in bob's window before anything is kept, keeping it puts the blip in both transcripts and clears the ghost and the pill goes dark when the server stops and comes back on its own when it returns. Enter sends from a composer and `cmd`/`ctrl` with it sends from either, which is the only way to keep a rewrite without reaching for the mouse: the rewrite is a textarea, where Enter is a new line.
