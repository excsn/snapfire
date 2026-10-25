# 111. Values the server and browser disagree on

The question this chapter answers: what happens to a component that reads the local clock, the time zone, `Math.random()` or `window` (values the server and the browser would compute differently) and why does it not end in a hydration error?

**For:** app developers, especially anyone coming from Next.js or Remix.

This chapter calls such a value a **browser-only value**: one whose right answer depends on the viewer's machine or the moment the browser runs, so a server cannot produce the string the browser will produce.

## The problem in other frameworks

A component like this is common:

```tsx
export default function Clock() {
  return <p>Rendered at {new Date().toLocaleTimeString()}</p>;
}
```

Next.js, Remix and React's own `renderToString` run that line in Node. The server writes its own time in its own zone, usually UTC in a container. The browser then runs the component again to hydrate it at a later moment in the viewer's zone and gets a different string. React 19 reports "Hydration failed because the server rendered text didn't match the client" and renders that part again in the browser; React 18 warned in development. Next.js lists this case among its common hydration errors and offers three fixes: move the read into `useEffect`, put `suppressHydrationWarning` on the element so the server's text stays until the next render or load the component with `dynamic(..., { ssr: false })` so the server renders nothing for it.

Each fix is written by hand after the error shows up.

## What fsr does

The build reads every component before anything runs, so it can tell a browser-only value from one the server can compute. It never renders a browser-only value on the server. What it does instead depends on where the value is used:

| The component | Where it runs | What you see |
| --- | --- | --- |
| Writes the viewer's clock as text: `<p>{new Date().toLocaleTimeString()}</p>`, a `const` holding it, a helper returning it, a prop a child writes as text | The server writes nothing for the value; the component hydrates with the value null, which matches, then computes it once mounted and renders again | A `browser` row in the report naming the line; the markup around the value is the server's |
| Uses a browser-only value anywhere else: in an attribute, in a condition, choosing markup, `Math.random()` shaping a list, `window` | The component renders in the browser | The report marks it `client` and names the line; the page around it is still server rendered |
| Calls `console.log`, `info`, `debug`, `warn` or `error` | The server logs on every render; the browser logs again when the component hydrates | A line under `fsr::console` in the server's log; the HTML is unchanged |
| Logs a browser-only value | The browser alone | No server line for that call; the component still renders on the server |
| Handles an event, `onClick={...}` | The browser | The server writes the markup and the browser attaches the handler |
| Runs an effect, `useEffect(...)` | The browser | Nothing on the server, as in React |
| Submits a form to a function, `<form action={save}>` | The browser | The server writes React's own placeholder `action`, so hydration matches |
| Passes a setter or `dispatch` on | The browser calls it | On the server it is a function that renders nothing, so `{dispatch ? "ready" : ""}` renders the same on both |

The first row is the `useEffect` fix Next's documentation has you write, written for you around the one expression that needs it and only after the build has checked the value lands as text or a prop rather than shaping markup. The second row does what Next's `ssr: false` does, for the one component that needs it. The rest are things React frameworks either run twice or leave you to arrange.

## Logging

A `console` call in a component's body is written to the server's log on every render:

```tsx
export default function Board({ squares }: { squares: string[] }) {
  const winner = calculateWinner(squares);
  console.log("board rendered, winner", winner);
  return <p>{winner ?? "no winner yet"}</p>;
}
```

```
INFO fsr::console: board rendered, winner null
```

The arguments are joined by spaces: a string as it is, a number or a boolean as JavaScript prints it, an object or an array as JSON. `log` and `info` are written at info, `debug` at debug, `warn` and `error` at their own levels. A call inside an `if` logs only when that branch runs. With no `fibre_logging.yaml` beside the configuration, the host writes warnings and errors plus everything under `fsr::console` from debug up to stderr, which is what `fsr dev` shows. A `fibre_logging.yaml` sets its own levels, so production can turn the target off.

A call that logs a browser-only value is left to the browser and the component still renders on the server:

```tsx
export default function Greeting({ name }: { name: string }) {
  console.log("Greeting was rendered at", new Date().toLocaleTimeString());
  return <h3>Hello, {name}!</h3>;
}
```

The server writes `<h3>Hello, Ada!</h3>` and no log line. The browser logs `Greeting was rendered at 14:02:11` in the viewer's time when the component hydrates. A component that never hydrates, one with no state and no handlers, never runs that call in the browser either, the same as a page Next renders statically.

One argument is still refused: a call to an extension only the server has, `id.new()` for one, since the browser would make the same call when the component hydrates and has no such extension.

## The viewer's clock as text

A browser-only value written as text under an element is left to the browser and the component still renders on the server:

```tsx
export default function Clock() {
  return <p className="at">Rendered at {new Date().toLocaleTimeString()}</p>;
}
```

The server writes `<p class="at">Rendered at </p>`. In the browser copy the build wraps the expression so it is null until the component has mounted: the first render matches the server's markup, so hydration has nothing to report and a second render right after mount computes the time and writes it. React's hydration never patches a text difference it is told to ignore, so this is the shape that works. The report says so:

```
browser   src/ui/Clock.tsx#Clock             src/ui/Clock.tsx:2:47: the current time on a render path, which the server and the browser read at different moments
```

The same holds for a `const` that holds the value, a helper that returns it and a prop a child writes as text: the parent computes it once mounted and the child renders again with it. The build only does this where the value is text or a prop, so the first render's markup is the server's. A browser-only value in an attribute, in a condition or choosing between markup sends the component to the browser instead:

```
rendered  src/ui/Clock.tsx#Clock             client      src/ui/Clock.tsx:2:34
client    src/ui/Clock.tsx:2:34              `.getHours()`, which reads the viewer's time zone
```

The page still renders on the server. `Clock` arrives as a React island with its props and draws itself in the browser, with no mismatch to report since the server sent no text for it. When the value does not have to be the viewer's, compute it on the server instead: `ctx.now` in the loader passed down as a prop, `toISOString()`, the `getUTC*` methods or `intl.date(when, "long")` from `@snapfire/fsr/client` under the request's locale. Then the component renders on the server and the browser computes the same string at hydration.

## The lab

Open the lowering conformance app's [`Board.tsx`](../../examples/conformance/lowering/app/src/idioms/Board.tsx) and run `fsr serve app` from `examples/conformance/lowering`. Load `/idioms`: the server's log shows `fsr::console: board rendered, winner null` once per request and the browser console shows the same line once the board hydrates.

Change the log line to `console.log("board rendered at", new Date().toLocaleTimeString())` and rebuild with `fsr build --verbose app`. The report still lists `Board` as `lowered`, the server's log has no line for it and the browser console prints the viewer's time.

Now render the time instead: `<p className="status">{new Date().toLocaleTimeString()}</p>` and rebuild the same way. The report still lists `Board` as `lowered` and adds a `browser` row for the line: the server writes the paragraph empty and the browser fills it once the board hydrates. `Clock.tsx` on the same page does this on purpose, with a currency and a weekday formatted on the server beside it. Make the value choose something instead, `<p className={new Date().getHours() > 6 ? "day" : "night"}>`: this app sets `[build] strict = true`, so the build refuses `Board` and names the line, since strict mode lets nothing fall back to the browser without saying so. Remove `strict` and rebuild: `Board` is `client`, the page around it is in the HTML and the board appears once the browser draws it. Put the line back.
