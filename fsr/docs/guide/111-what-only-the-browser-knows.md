# 111. What only the browser knows

The question this chapter answers: what happens to code that reads something only the viewer's browser has (the local clock, the time zone, a random number, `window`) and why does it not cost a hydration error?

**For:** app developers, especially anyone coming from Next.js or Remix.

## The problem every server renderer has

A component like this is common:

```tsx
export default function Clock() {
  return <p>Rendered at {new Date().toLocaleTimeString()}</p>;
}
```

Next.js, Remix and React's own `renderToString` run that line in Node. The server writes its own time in its own zone, usually UTC in a container. The browser then runs the component again to hydrate it at a later moment in the viewer's zone and gets a different string. React 19 reports "Hydration failed because the server rendered text didn't match the client" and renders that part again in the browser; React 18 warned in development. Next.js lists this case among its common hydration errors and offers three ways out: move the read into `useEffect`, put `suppressHydrationWarning` on the element so the server's text stays until the next render or load the component with `dynamic(..., { ssr: false })` so the server renders nothing for it.

Each of those is something you write by hand, after the error shows up.

## What fsr does instead

The build reads every component before anything runs, so it knows which values only the browser can produce. It does not compute a wrong one. It decides per component and per call:

| What the code does | Where it runs | What you see |
| --- | --- | --- |
| Renders a value only the browser knows: `toLocaleTimeString()`, `getHours()`, `Date.now()`, `Math.random()`, `window` | The component renders in the browser | The report marks it `client` and names the line; the page around it is still server rendered |
| Logs with `console.log`, `info`, `debug`, `warn` or `error` | The server logs on every render, the browser logs again when the component hydrates | A line under `fsr::console` in the server's log; the HTML is unchanged |
| Logs a value only the browser knows | The browser alone | No server line for that call; the component still renders on the server |
| Handles an event, `onClick={...}` | The browser | The server writes the markup and the browser attaches the handler |
| Runs an effect, `useEffect(...)` | The browser | Nothing on the server, as in React |
| Submits a form to a function, `<form action={save}>` | The browser | The server writes React's own placeholder `action`, so hydration matches |
| Holds a setter or `dispatch` and passes it on | The browser calls it | On the server it is a function that renders nothing, so `{dispatch ? "ready" : ""}` agrees with the browser |

The first row is Next's `ssr: false`, chosen for you and only for the component that needs it. The rest are things React frameworks either run twice or leave you to arrange.

## Logging

`console` calls on a component's render path lower to a log the server writes on every render:

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

The arguments are written joined by spaces: a string as it is, a number or a boolean as JavaScript prints it, an object or an array as JSON. `log` and `info` are written at info, `debug` at debug, `warn` and `error` at their own levels. A call inside an `if` logs only when that branch runs. With no `fibre_logging.yaml` beside the configuration, the host writes warnings and errors plus everything under `fsr::console` from debug up to stderr, which is what `fsr dev` shows; a `fibre_logging.yaml` decides for itself, so production can turn the target off.

When a log argument is something only the browser knows, the server leaves the call out and the component still renders on the server:

```tsx
export default function Greeting({ name }: { name: string }) {
  console.log("Greeting was rendered at", new Date().toLocaleTimeString());
  return <h3>Hello, {name}!</h3>;
}
```

The server writes `<h3>Hello, Ada!</h3>` and no log line. The browser logs `Greeting was rendered at 14:02:11` in the viewer's time when the component hydrates. A component that never hydrates, one with no state and no handlers, never runs that call in the browser either, the same as a page Next renders statically.

One argument is still refused: a call to an extension only the server has, `id.new()` for one, since the browser would run the same call when the component hydrates and has no such extension.

## Rendering a value only the browser knows

A value the viewer's browser alone can produce, rendered into the markup, sends that component to the browser:

```
rendered  routes/page.tsx#default            lowered
          src/ui/Clock.tsx#Clock             client      src/ui/Clock.tsx:2:34
client    src/ui/Clock.tsx:2:34              `.toLocaleTimeString()`, which reads the viewer's time zone
```

The page still renders on the server. `Clock` arrives as a React island with its props and draws itself in the browser, with no mismatch to report since the server sent no text for it. When the value does not need to be the viewer's, read it on the server: `ctx.now` in the loader passed down as a prop, `toISOString()`, the `getUTC*` methods or `intl.date(when, "long")` from `@snapfire/fsr-client/std` under the request's locale. Then the component renders on the server and the browser reads the same string at hydration.

## The lab

Open the lowering conformance app's [`Board.tsx`](../../examples/conformance/lowering/app/src/idioms/Board.tsx) and run `fsr serve app` from `examples/conformance/lowering`. Load `/idioms`: the server's log shows `fsr::console: board rendered, winner null` once per request and the browser console shows the same line once the board hydrates.

Change the log line to `console.log("board rendered at", new Date().toLocaleTimeString())` and rebuild. The report still lists `Board` as `lowered`, the server's log stays quiet for it and the browser console prints the viewer's time.

Now render the time instead: `<p className="status">{new Date().toLocaleTimeString()}</p>`. Rebuild with `[build] strict = true` set, as this app has it: the build refuses `Board` and names the line, since strict mode allows nothing to fall back to the browser silently. Remove `strict` and rebuild: `Board` is `client`, the page around it is in the HTML and the board appears once the browser draws it. Put the line back.
