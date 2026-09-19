# 105. No framework at all

The question this chapter answers: what does an fsr application look like with no component framework in it, where does the interactivity go and how does a region talk to the server without a payload or an island?

**For:** app developers.

## The browser is the mounter

Chapter 104 put a Vue component where a React one had been and the seam held: a template places it, the server writes its marker, a mounter takes it in the browser. The cheapest case on that spectrum has no mounter. A custom element is defined once by a module the browser runs and upgraded wherever the parser finds its tag. The server can write that tag and everything inside it, because it is markup like any other; the browser does the rest when the definition arrives.

The tool library, [`toolshed_web_ts`](../../examples/toolshed_web_ts/README.md), is built that way. Every route is a template, every interactive piece is a `.ts` file under `src/elements/` calling `customElements.define`; the regions that reach the server are htmx attributes. Nothing mounts, nothing hydrates and the one line in `generated/islands.ts` registers an element definition rather than a component. The bundle is `src/**/*` and two generated files; the import map is the client, its store and htmx.

## Writing a custom element in a template

A template writes the element the way it writes a `div`, with its light DOM inside:

```tsx
<shed-tally count={reserved}>
  <button type="button" className="tally" aria-label="reserved" aria-expanded="false">
    {reserved} reserved
  </button>
  <div className="tally-panel" hidden>
    <p>Reservations are kept in the session cookie.</p>
  </div>
</shed-tally>
```

The lowerer takes a hyphenated tag as an element and its attributes as attributes, so the server renders the button with the count in it before any script runs. The dialect's declarations, `@snapfire/fsr-authoring/template`, type a hyphenated tag as a custom element whose attributes are its own, so `<divv>` is still the typo the checker catches while `<shed-tally count={n}>` passes. An attribute the declarations do not name takes a scalar, children, a style object or a handler. An array of objects fails the typecheck, since an attribute on an element nothing hydrates is text. An `hx-get` on an anchor passes for the same reason any hyphenated attribute does.

The element's module wires what the server wrote:

```ts
import { get, subscribe } from "@snapfire/fsr-client/store";
import { reservedCount } from "../store.js";

class ShedTally extends HTMLElement {
  #stop: (() => void) | null = null;

  connectedCallback(): void {
    const button = this.querySelector<HTMLButtonElement>("button.tally");
    const panel = this.querySelector<HTMLElement>(".tally-panel");
    if (!button || !panel) return;
    const toggle = () => {
      panel.hidden = !panel.hidden;
    };
    const show = (count: unknown) => {
      if (typeof count === "number") button.textContent = `${count} reserved`;
    };
    button.addEventListener("click", toggle);
    show(get(reservedCount));
    const unsubscribe = subscribe(reservedCount, show);
    this.#stop = () => {
      button.removeEventListener("click", toggle);
      unsubscribe();
    };
  }

  disconnectedCallback(): void {
    this.#stop?.();
    this.#stop = null;
  }
}

customElements.define("shed-tally", ShedTally);
```

`get` and `subscribe` are the whole store adapter. React reads the store through a hook and Vue through a ref, because each framework has its own idea of a reactive value; an element has none, so it reads the value once and takes a callback for the changes. The `get` comes first because `subscribe` only hears what changes after it: the server wrote this count into the button, but a property or anything else the server did not write would stay empty until the key next moved. The layout's loader seeds the key with `export const store`, exactly as it would for a React island; the count follows the store from then on.

`disconnectedCallback` removes the listener and the subscription that `connectedCallback` added. A morph that moves an element disconnects it and connects it again, so without the stop a moved tally would carry two click listeners that cancel each other out; a removed one would stay subscribed and keep being written.

## A shadow root the server writes

An element that wants its own styles has a shadow root. The parser attaches one from a `<template shadowrootmode="open">` inside the element, before any script runs. The element's template lives beside its class in `elements/loan-planner.tsx`. The server writes it inside every `<loan-planner>` it renders:

```tsx
export default function LoanPlanner({ deposit, days, max, disabled }: { deposit: number | bigint; days: number; max: number | bigint; disabled?: boolean }) {
  return (
    <>
      <style>{":host { display: block } :host([disabled]) { background: #f7f8fa } output { font-weight: 600 }"}</style>
      <label>
        {disabled ? "Borrowed for" : "Borrow for"}
        <input type="range" name="days" min="1" max={`${max}`} value={`${days}`} disabled={disabled} />
        <output>{days} days</output>
      </label>
    </>
  );
}
```

The tool page places the tag and nothing else, `<loan-planner name="days" deposit={tool.deposit} disabled={reserved} max={tool.days} days={days} />`. The file name is the tag. The template's props are the element's attributes, so an array or an object reaches the template without becoming an attribute. The build types the tag with those props in `generated/elements.d.ts`, so a missing `days` or a `max` of the wrong type fails the typecheck at the page, while `name`, which the class reads and the template does not take, passes. A template with state or a handler stops the build, since the class owns the behaviour.

The planner is styled and laid out from the first paint, with no framework and no stylesheet to link. One thing to know: `innerHTML` attaches no declarative shadow roots, so an element that arrives inside an htmx swap finds its template as an ordinary child. `shadowOf` from `@snapfire/fsr-client/elements` answers both cases, the root the parser attached or one attached from the template:

```ts
const root = shadowOf(this, this.#internals);
```

The navigator writes what it applies with `setHTMLUnsafe` where the browser has it, so a page reached by a link keeps its shadow roots as well.

The root is open unless the template declares its own. Returning a `<template shadowrootmode="closed">` makes the planner's root closed. `shadowrootdelegatesfocus` on it hands focus to the range when the host is focused:

```tsx
return (
  <template shadowrootmode="closed" shadowrootdelegatesfocus>
    <style>{":host { display: block } output { font-weight: 600 }"}</style>
    <label>
      Borrow for <input type="range" name="days" min="1" max={`${max}`} value={`${days}`} />
    </label>
  </template>
);
```

The build reads that `<template>` as the shadow root. Anything on it besides the four `shadowroot` attributes stops the build, since the parser drops the template element once it becomes the root. A closed root is not on `this.shadowRoot`. The planner already holds its internals for the form value, which is why the call above passes `this.#internals`. An element without them would get `null` from `shadowOf`.

The state is in the markup, not in a script that runs after paint. `reserved` comes from the loader, so the server writes `disabled` on the host and on the range, with the label reading "Borrowed for" from the first byte. A boolean attribute is written bare when it is true and left out when it is false, which is what `:host([disabled])` and a disabled control each want.

## A value inside a shadow root that the form posts

A control in a shadow root has no form owner, so the slider is not submitted by the form around it however it is nested. An element that wants to be a field says so and supplies its own value:

```ts
class LoanPlanner extends HTMLElement {
  static formAssociated = true;
  #internals = this.attachInternals();

  #show(range: HTMLInputElement) {
    this.#internals.setFormValue(range.value);
  }
}
```

With `name="days"` on the host, `days` is in the posted body under that name, natively and through htmx alike, since both build the form data from the form. The action reads it as it reads any other field, clamps it to what the shed lends that tool for and writes the agreed length into the session, so the page that comes back says what was actually agreed rather than what the tool's limit is.

## A definition that waits until its element is in view

Everything above is defined when the entry module runs, which is what a masthead wants and not what a panel at the bottom of the page wants. An element has no mount, so what a timing can defer is its definition. That is what a template asks for:

```tsx
<Island when="visible" define="@src/elements/time-ago.ts">
  <ul>
    {loans.map((loan) => (
      <li key={loan.tool}>
        <time-ago datetime={loan.back}>back {loan.back}</time-ago>
      </li>
    ))}
  </ul>
</Island>
```

The child is an element, not a component. The server writes its markup inside the island marker, so the list is readable with no script at all: `back 2026-09-14` until the definition lands, `back in 2 days` after it. The build registers the module with `defineMounter`, whose mount does nothing, so the island machinery imports it when the panel scrolls into view and every element inside upgrades itself. The module needs an `export` to be imported dynamically; the class is the obvious one.

Islands in this application need nothing more than that: one marker, no props script, no mounter and no framework.

## A region that asks the server for markup

An island round trip carries a payload the client applies. htmx carries none: an attribute names a URL, the response is markup and it is swapped into a target. What it needs from the host is one segment of a route as HTML with nothing around it. That is what `__fragment` in the query asks for:

```text
GET /?category=Garden&__fragment        the page segment, filtered by the query it carried
GET /tool/3?__fragment=loans            the parallel slot named loans, wherever it sits on the route
GET /?__fragment=nope                   404, no slot named `nope` on this route
```

The host renders the whole route for either, layouts included, waits for every deferred segment rather than streaming a fallback, picks the segment out of the tree and writes it with no shell, no segment delimiters and no sidecar. Loaders never see the key; `ctx.query` and the segment keys are what they would be for a document. The shelf chips are that request as attributes:

```tsx
<a href="/?category=Garden" hx-get="/?category=Garden&__fragment" hx-target="closest .page" hx-swap="outerHTML" data-sf-native>
  Garden
</a>
```

`data-sf-native` tells the navigator this anchor is not its own. The loans panel polls the same way, `hx-get="?__fragment=loans"` with `hx-trigger="every 15s"`; the `<time-ago>` elements inside each fresh fragment upgrade as they land, since the browser defines the element once and applies it everywhere.

## A form that gets its page back

An action posted as a form is answered with a redirect to the page that posted it; the tera application in the examples posts its forms that way with no JavaScript at all. When the action's URL carries `__fragment`, the redirect carries it too:

```tsx
<form method="post" action="/_sf/action/tool.$id.reserve" hx-post="/_sf/action/tool.$id.reserve?__fragment" hx-target="closest .page" hx-swap="outerHTML">
  <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
  <input type="hidden" name="tool_id" value={tool.id} />
  <loan-planner name="days" deposit={tool.deposit} disabled={reserved} max={tool.days} days={days} />
  <button type="submit">Reserve it</button>
</form>
```

htmx posts, follows the 303 to `/tool/3?__fragment` and swaps in the tool page rendered from the session the action just wrote, with the button now saying the opposite. Without JavaScript the same form posts natively and lands back on the document. `csrf_token` is a prop every page and layout may read, minted for anonymous sessions too when `[session] csrf = "always"` is set, which a form anonymous visitors post needs.

## Two libraries, one document

A fragment ends with the same inert seed script a document carries, so the store follows the server through htmx exactly as it does through a payload. Making the two libraries aware of each other is one line:

```ts
import htmx from "htmx.org";
import { boot, enableNavigation } from "@snapfire/fsr-client";
import { bindHtmx } from "@snapfire/fsr-client/htmx";

boot();
enableNavigation();
bindHtmx(htmx);
```

`fsr use app htmx` writes the map line, vendors htmx and prints those three lines for `main.ts`; `fsr new --with htmx` writes them into the scaffold's own. htmx is passed in rather than imported by the client, so the binding takes whatever version the import map names. Both directions are needed, which is what that one line wires up. After htmx swaps, `adopt` reads the seeds nothing has read yet, which is how the masthead count moves for a page the layout was never re-rendered for; `scan` would mount any island the fragment placed. After the navigator applies a payload it dispatches `sf:navigate` plus `sf:fill` for each deferred segment it fills, so htmx processes the markup the navigator wrote. Leave that second direction out and a reserve form reached by clicking a tool name is markup htmx never saw: the browser posts it natively and the document reloads. That is the one way this arrangement fails. It fails visibly.

## The lab

Run `fsr build app` in the tool library and read `app/generated/islands.ts`: one registration, an element definition with `defineMounter`, no mounter imported from any framework. Read the report: every route module is `static`. View the source of `/tool/3` before the scripts run: the planner's shadow root is there inside its `<template>`, the form carries the token and the only `<sf-i>` on the page is the loans list, holding the markup its definition will upgrade.

Ask for fragments with `curl`, as above. The page fragment starts at `<section` and ends with the seed script; the weather slot is its error boundary alone; the unknown slot is one line.

Open the shelves in a browser, open the tally panel, click a tool and reserve it. The masthead was never touched and the panel is still open; the count moved because the fragment carried the seed. Then take the `sf:navigate` listener out of `main.ts`, rebuild and do it again: the document reloads on the reserve. Put it back.

Add a `useState` to `routes/page.tsx` and build: the page stops being `static`, so the registry would mount it through React. The build stops with the error chapter 104 shows, since this import map has no React either. The rule from chapter 104 is the same rule here.
