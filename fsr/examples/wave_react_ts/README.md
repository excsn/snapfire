# wave_react_ts

Google Wave's idea, on FSR: a conversation of blips nested inside the blips they answer, participants you can see arriving, and everyone's typing visible to everyone else before a word of it is kept.

It is the example that needs a WebSocket, and it is the one that shows exactly how much a WebSocket is for. A blip is durable and never touches the socket. A keystroke is not durable and never touches an action.

## Running it

```sh
cargo run -p wave_react_ts
```

Open `http://127.0.0.1:8140/` in two windows, name yourself in each, open the same wave in both and start typing in one.

## The two halves

| What | Seam | Why |
| --- | --- | --- |
| A blip, kept for good | an action, then `Host::publish("wave/<id>")` and `live()` | it belongs in the transcript, so the loader is what answers for it |
| Who is here | the socket, `On::Joined` and `On::Left` | it is true only while a connection is |
| What someone is typing | the socket, one row per keystroke | it is superseded by the next keystroke and worth nothing after |

The socket carries rows, `{"key": ..., "value": ...}` up and `{"rows": [...]}` down, and the browser writes each row into the store, so `Presence` and `Under` follow by reading a key. Neither ever fetches.

## What is server-rendered and what is not

The transcript is. Every blip, in reading order, at the depth `waves.getWave` gave it, rendered in Rust with no JavaScript engine, so the wave is readable before a line of the bundle has run.

Three things are islands, because they cannot be rendered ahead of time: `Presence`, the pill and the participant list; `Under`, the drafts and the composer beneath each blip; and `Name`. Everything else on the page is lowered. `fsr check app` prints the list and will tell you the moment something falls out of it.

## A wave is not public

`wave/<id>` is followed only by a session that has opened that wave, which the wave's own loader records. The same rule guards the stream and the socket, so neither is a way around the other, and a request for a wave you have not opened is 403.

## What it is worth reading for

**One connection per page, not per island.** `src/ui/wire.ts` holds the socket and the `live` stream in a module and hands out shares. Islands come and go as the transcript re-renders; the connection does not, and presence does not flicker.

**A store key the build can read.** The rows are `wave/here` and `wave/drafts`, not `wave/<id>/here`. A `useStore` key built from a prop cannot be lowered, so the island falls to the browser and takes the page with it. The key names what the page is showing rather than which wave, since a document shows one.

**Ghosts sit in their own element.** They used to be siblings of the composer, and React reconciles children by position: someone else starting to type shifted the composer down a slot, React remounted it, and the draft in it was lost. Wrapping them in one `<div className="ghosts">` keeps the composer's position fixed, and a draft survives whatever anyone else is doing.

**What the build refuses.** `.slice()`, `[...string]` and `Boolean()` each cost this page its server rendering while they were in it, one at a time, each named with its line by `fsr check`. None of them is an error; each is a component quietly moving to the browser.

## Tests

`cargo test -p wave_react_ts`: that a wave renders its blips in reading order with the depth of each, that keeping a blip nests it, names the wave as a topic and adds whoever wrote it, that neither the stream nor the socket is open to a session that has not opened the wave, that the field answers a join with who is here and forgets a leave, and that a draft reaches everyone but its author and goes when it empties or its author does.

`fsr test app`: the depths, which parts are islands and the inbox beside the open wave.

Checked in two browsers: presence lights up in both, alice's keystrokes appear under the right blip in bob's window before anything is kept, keeping it puts the blip in both transcripts and clears the ghost, Enter sends, and the pill goes dark when the server stops and comes back on its own when it returns.
