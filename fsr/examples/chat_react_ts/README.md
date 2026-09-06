# chat_react_ts

Rooms, messages and the message someone else sent arriving on your screen. It is the smallest application that needs the server to say something to a page nobody asked, which is what `Host::publish` and `live()` are for, and it needs no WebSocket to do it.

## Running it

```sh
cargo run -p chat_react_ts
```

Then open `http://127.0.0.1:8130/` in two windows, name yourself in each, open the same room in both and say something in one.

## How it works

Sending is an action. `say` posts the message through the `rooms` service, which keeps it and names the room as a topic; the host publishes `room/<id>`; every open `/_sf/live?topics=room/<id>` stream hears it; `live()` in each browser calls `refresh()`, so the route's loader runs again and the transcript is patched in place. Nothing polls, nothing reloads, and the stream carries the topic and never the message: the loader answers what was said, the ordinary way.

| Piece | What it is |
| --- | --- |
| `routes/page.tsx` | the room list, and the form that names you |
| `routes/room/[id]/page.tsx` | the transcript, the composer and the island that follows the room |
| `routes/room/[id]/actions.ts` | `say`, the only way a message is made |
| `src/ui/Follow.tsx` | one `live(["room/" + id])` in an effect |
| `src/backend.rs` | the rooms in process, and the topic each message names |

## A topic is not public

`room/lobby` is a private thing to follow, so the host is given a rule:

```rust
builder.topics(|topic, session, _| match topic.strip_prefix("room/") {
  Some(room) => matches!(session.get("rooms"), Some(Value::Map(open)) if open.contains_key(room)),
  None => false,
})
```

Opening a room is joining it: the room's loader records it in the session, and the rule reads that when the page asks to follow. A session that has opened no room is refused with 403, and so is a room it never opened. Without a rule any topic may be followed by anyone, which is right for a board on a wall and wrong for a room.

## What it is worth reading for

Both forms are uncontrolled: the submit handler reads `FormData` off the form rather than holding every keystroke in React state. That is what a browser expects, it lets a password manager and a test driver fill the field, and for a composer it is less code.

The name is in the session, not in a token: `session.name` is set by an action and read by the layout's loader, and `app/schemas/session.ts` is what types it, which is why `session.name` is a `string` and not `unknown`.

`autoComplete` is absent from both inputs on purpose. With it, the specs fail on a React hydration warning that says the server rendered nothing there, and the server did render it: linkedom, which `fsr test` runs the DOM on, implements no `autocomplete` property for React to compare against. The same page hydrates in Chrome with an empty console. Setting an attribute linkedom does not model costs a false failure, so the two chat inputs go without.

## Tests

`cargo test -p chat_react_ts`: that a room renders its transcript and marks what this reader said, that opening a room joins it and the topic rule reads that, that saying something keeps it and names the room as a topic while no other room hears it, and that the room list counts what is in each room.

`fsr test app`: the transcript, the composer and the live island, and the room list's links.

Checked in two isolated browsers: alice says something and it appears in bob's window, and bob's reply appears in alice's, with `performance.getEntriesByType("navigation").length` still 1 in both and both consoles empty.
