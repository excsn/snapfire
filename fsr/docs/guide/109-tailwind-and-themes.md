# 109. Tailwind and themes

**For app developers.** How a stylesheet that imports Tailwind gets its utilities from the classes your pages render, what the build says about a class it cannot see, how a theme is chosen for the whole document per request and how a card image draws the same classes.

## Tailwind is an import

A stylesheet under `styles/` that imports Tailwind is the opt-in:

```css
/* app/styles/app.css */
@import "tailwindcss";

@theme {
  --color-accent: #1f6feb;
  --color-line: #dde3ea;
}

@utility pill {
  @apply inline-flex items-center rounded-full border border-line px-2.5 py-0.5 text-[13px];
}
```

Everything Tailwind CSS v4 reads in a stylesheet works: `@theme`, `@utility`, `@variant`, `@custom-variant`, `@apply`, `@source inline(...)`, `theme()` and `--spacing()`. There is no `tailwind.config.js` and no Node: the engine is `snapfire_tailwind`, a Rust port of Tailwind's own compiler. A JavaScript plugin or config is refused by name.

## Where the classes come from

Tailwind's CLI scans your files for anything that looks like a class. fsr already knows where every class is, so it lists them instead. `fsr build` reads the lowered plan: every `class` an element renders, both branches of a conditional, the fixed parts of a template, the values of a literal lookup object and the `className` a component is placed with. It writes them to `generated/tailwind.classes`, one per line with the modules that use it. The bundle builds the stylesheet for exactly that set:

```text
tailwind  51 classes in generated/tailwind.classes
```

A component that renders in the browser alone has no lowered tree, so the bundle reads its source instead: `className` and `class` attributes and the arguments of `clsx`, `cn` and Vue's `normalizeClass`. In `fsr dev` a save that changes the set rebuilds the stylesheet with it.

## A class the build cannot list

A class built from data at runtime is a class the build never sees:

```tsx
<span className={`rounded-full px-2 status-${system.state}`}>{system.state}</span>
```

The build says so and names the module. The fix is to write the values where the build can read them. A lookup object is read whole:

```tsx
const tone: Record<string, string> = {
  operational: "bg-emerald-100 text-emerald-800",
  degraded: "bg-amber-100 text-amber-800",
  down: "bg-rose-100 text-rose-800",
};

<span className={`rounded-full px-2 ${tone[system.state]}`}>{system.state}</span>
```

A value that is not a class at all is better as an attribute and a variant:

```tsx
<tr data-status={invoice.status} className="group">
  <td className="group-data-[status=overdue]:text-rose-600">{invoice.status}</td>
</tr>
```

When neither fits, list the values in the stylesheet:

```css
@source inline("bg-{red,amber,emerald}-100");
```

A class that is not a utility and that no stylesheet defines is a warning too, so a typo shows up in the build rather than as a missing style:

```text
⚠️  `flx` in routes/page.tsx#default is not a Tailwind utility and no stylesheet defines it
```

## Tailwind in a site

A site's stylesheet is linked after the shell's in the same document, so a site brings its utilities and leaves the base styles to the shell:

```css
/* sites/blog/app/styles/blog.css */
@import "tailwindcss/theme" layer(theme);
@import "tailwindcss/utilities" layer(utilities);

@theme reference {
  --color-card: #ffffff;
  --color-accent: #1f6feb;
}
```

`@theme reference` names the shell's tokens without writing them: `bg-card` compiles to `background-color: var(--color-card, #fff)`, so the shell's value and the document's theme decide what the site's card looks like.

## A theme is the whole document

A theme is a file of `@theme` blocks under `styles/themes/`, named by the file:

```css
/* app/styles/themes/ocean.css */
@theme {
  --color-accent: #0e7490;
  --color-canvas: #f0f9fb;
}
```

Every theme is in the stylesheet as `[data-sf-theme="ocean"] { ... }`, written after the root theme, so switching is one attribute on `<html>`. The generated types declare `Theme` as the union of the names, a site's own and its shell's.

A layout or a page picks one with a literal export, plain or forced:

```tsx
export const theme = "editorial";
export const theme = { name: "ledger", force: true };
```

Down a route's layouts the deepest one that sets a theme wins, unless a level above it forces its own; of two forced levels the outer one wins. In the portal the blog sets `editorial` and billing forces `ledger`. A name with no file behind it fails the build.

## Choosing per request

`app/theme.ts` decides for each request, after middleware and before the page. It is lowered like a loader, so it reads the session, the query and the identity. `ctx.theme` is the route's own choice:

```ts
import type { Scheme, Theme, ThemeChoice, ThemeCtx } from "@snapfire/fsr";

export function theme({ theme, query, session }: ThemeCtx): ThemeChoice {
  const scheme = session.scheme === "light" || session.scheme === "dark" ? (session.scheme as Scheme) : null;
  if (theme.forced) return { name: theme.name, scheme };
  const picked = query.theme ?? session.theme;
  return { name: picked === "" ? theme.name : (picked as Theme), scheme };
}
```

`force` is a flag the body respects rather than a rule the host enforces, so a site that must keep its brand says so and the body keeps it. A mounted site's own `theme.ts` runs after the shell's and sees the shell's answer as `ctx.theme`. The answer is written on the document as `<html data-sf-theme="ocean" data-sf-scheme="dark">`; a client navigation sets both from the payload's head row, so moving from the portal into billing switches the theme without a reload. A prerendered page is written without one; the host writes the request's theme into its `<html>` before sending it, the same step as for a page rendered live.

A route whose forced theme was kept also carries `data-sf-theme-forced` on `<html>`, so a shell's own theme picker can say so with a variant alone:

```tsx
<span className="hidden [[data-sf-theme-forced]_&]:inline">This site keeps its own theme</span>
```

Light and dark are the scheme, not a theme, so a brand theme and the visitor's scheme combine. Key the variant on the attribute and give the dark values a selector that outranks a theme's:

```css
@custom-variant dark (&:where([data-sf-scheme="dark"], [data-sf-scheme="dark"] *));

@layer theme {
  :root[data-sf-scheme="dark"] {
    --color-canvas: #0d1117;
    --color-ink: #e6edf3;
  }
}
```

A theme never reaches a body, so it changes no cache key.

## Card images draw the same classes

A card image is drawn by a renderer that matches no selectors, so the build turns each class into plain declarations in `generated/tailwind.cards.json`, with colors in sRGB and variables resolved. The host writes them into each element's style:

```tsx
export default function Image({ post }: PostSlugProps) {
  return (
    <div className="flex h-full w-full flex-col justify-between bg-amber-50 p-[72px] text-gray-800">
      <h1 className="m-0 text-[72px]/[1.1] font-bold">{post.title}</h1>
    </div>
  );
}
```

An inline `style` still wins over a class, as on a page. A breakpoint variant applies when the card is at least that wide; `hover:`, `dark:` and the other variants mean nothing on a still image and are skipped.

## Lab

In the portal example, open `/?theme=forest`: the whole document turns green and serif for that one request. Sign in, pick Ocean and Dark on `/account` and follow the header into the blog, billing and status. The blog takes your pick over its own `editorial`, billing keeps `ledger` and every site follows the dark scheme. Then change the status page's lookup to `` `status-${system.state}` `` and run `fsr build sites/status/app`: the report names the module and the value it cannot list.
