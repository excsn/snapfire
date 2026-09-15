# 106. Two frameworks on one page

The question this chapter answers: what does it actually take to run React and Vue in the same document, what do they share and what does it cost?

**For:** app developers, plus anyone weighing a migration.

## The seam is a module id

A placement carries a module id and nothing else. The server writes `<sf-i data-sf-module="js/src/ui/Holdings.vue#default">` around the markup it rendered; the browser looks that id up in the registry and calls whatever mounter the entry names. Nothing in the plan, the payload or the renderer knows which framework is behind an id, which is why a page can hold more than one.

That makes mixing a property of the registry rather than a feature:

```ts
registerIsland("js/src/ui/Watch.tsx#default", { loader: () => import("./ui/Watch.js").then((m) => m.default), mount: reactMounter, patch: reactPatcher, unmount: reactUnmounter });
registerIsland("js/src/ui/Holdings.vue#default", { loader: () => import("./ui/Holdings.vue"), mount: vueMounter, patch: vuePatcher, unmount: vueUnmounter });
```

The [`uni`](../../examples/uni/README.md) example is a page doing exactly that: a Tera layout with a React island in its masthead, a Vue island on the board beneath it and an htmx region beside them, all from one payload.

## One store, two adapters

Each framework reads the store through its own adapter, both adapters being the same store. The key is declared once and imported by both:

```ts
export const watchedKey = key<string>("uni/watched");
```

React takes it as state:

```tsx
const [held, setHeld] = useStore(watchedKey, symbol);
```

Vue takes it as a ref:

```ts
const held = useStore(watchedKey, props.watched);
```

Clicking a row in the Vue table writes `held.value` and the React masthead re-renders with the new symbol. Neither component imports the other; neither knows what the other is written in. The server renders both with the same value as a prop, so the first paint agrees with the store before any script runs.

## One router over segments that differ

A segment is a segment. `uni` puts the Vue island on `/board` and a React island on `/news`, both under the same layout, so clicking between them replaces one framework's segment with the other's while the layout, the masthead island included, is kept with its state. The navigator does not consult a framework to do it: it applies the payload's segments by key, hands each region's props to whatever is mounted there and mounts what is not.

## The third shape

htmx belongs in this chapter because it is not a third runtime. It has no build step, no mounter and nothing to hydrate: an attribute names a URL, the response is markup and it is swapped in. What it asks of the framework is a fragment, which chapter 105 covers, plus one line to keep the two libraries aware of each other:

```ts
bindHtmx(htmx);
```

So one page here holds three interaction models: a component the browser hydrates, a component the browser mounts fresh, a region nothing mounts at all. The lot stepper in the masthead is a fourth, from chapter 102: a React component the host renders and steps itself, placed in server mode, whose buttons call an action the host dispatches inside the round trip.

The fifth is the cheapest to serve and it holds the page's most valuable content. The book under the masthead, value against cost with the profit between them, is arithmetic the layout's loader did, printed straight into the markup: no island, no mounter, no props script, nothing to hydrate, no module loaded. It is current because the desk's clock pushes `prices` and `live(["prices"])` revalidates the route, so the server computes the numbers again and the navigator swaps the segment. Reach for an island when the browser owns something. When the server owns it, plain markup is enough.

## What it costs

This is the part worth reading before you reach for it. Measured from the files `uni`'s board actually loads, gzip at level 9:

| | raw | gzip |
| --- | --- | --- |
| React, with its adapter | 153.5K | 49.9K |
| Vue, with its adapter | 116.9K | 45.8K |
| htmx, with its binding | 165.3K | 37.0K |
| the fsr client | 60.8K | 17.6K |
| this application | 6.6K | 2.2K |
| everything the page loads | 503.1K | 152.6K |

React and Vue together are 96K compressed before a line of application code runs. A page that needs both pays for both, every visit; no amount of seam design makes that cheaper.

Where it earns its keep is narrow and worth naming:

- **A migration.** Moving off one framework island by island rather than in one jump, with both running until the last one is gone.
- **Two teams, one page.** A shell one group owns holding a panel another group owns, without agreeing on a framework first.
- **Zero-runtime pieces.** Custom elements are the browser and compile away to nothing, so dropping one into a React page costs what chapter 105 measures rather than what this table does.

What it is not is a default. If one framework will do, use one.

## The lab

Run the example: `cargo run -p uni`, then build its browser tree the way its README says, since it keeps `js/` rather than `app/`.

Open `/board` and click a row in the table. The masthead symbol changes; the table is Vue and the masthead is React. Open the console: nothing.

Click News. The page segment is React now, the masthead is the same element it was, the feed's filter already reading the symbol you picked in the Vue table. Click Board again: the table comes back with your row still held.

Watch the tape on the right for ten seconds. It swaps itself, having asked the host for one slot of the route. Nothing in it mounted.

Watch the book while you wait. The prices move every four seconds, the push revalidates and the value and the profit follow, all of it markup. Press "buy 10" and the cost climbs by what the shares cost at that moment rather than by what the desk paid years ago, which is why a position carries its own basis.

Press the lot's `+`. Nothing mounted for that either: the click posts to the host, Rust runs the lowered handler, the handler calls `desk.lot`, the host runs the action and answers the stepper's new markup, then the page refreshes its data and the masthead's buy button, a React island, reads the size the action wrote. Reload: the lot is still what you set, since the action wrote the session. [`Lot.tsx`](../../examples/uni/js/src/ui/Lot.tsx) is a component with two buttons calling an action and nothing else; the mode on its placement in `layout.tera` is what decides that the handler runs in Rust.

Take `bindHtmx(htmx)` out of `js/src/main.ts`, rebuild and navigate to News and back: the tape stops polling on the page the navigator wrote, because htmx never saw that markup.
