# 305. Third-party scripts

The question this chapter answers: where do an analytics tag, a consent banner, a font and a favicon go, given that an application has no document template, no head component and writes no `<script>` tag?

**For:** app developers.

## Nothing in the head is written as HTML

A conventional site keeps a document template with the fixed part of the head in it: the base, the robots meta, a preconnect to the font host, the icons, the analytics tag. fsr has no such template. The host writes the document (chapter 200) and what it writes is data from two places: what it inferred from the app directory and what the routes said in their `meta` exports (chapter 100). There is no third place and no configuration key that takes an element.

So the fixed part of the head is the root layout's `meta`. It runs for every route beneath it, an inner route overrides one row at a time by naming the same element and the rows are data the build can read. The helpers in `@snapfire/fsr/head` spell the common ones; an element they do not spell is an object literal with a `tag` and its attributes:

```ts
import { linkTag, metaTag, robots } from "@snapfire/fsr/head";

export const meta = () => ({
  head: [
    { tag: "base", href: "/" },
    robots("index,follow"),
    metaTag("format-detection", "telephone=no"),
    linkTag("preconnect", "https://fonts.example"),
    linkTag("stylesheet", "https://fonts.example/inter/inter.css"),
    { tag: "link", rel: "icon", type: "image/png", sizes: "32x32", href: "/static/favicon-32x32.png" },
    { tag: "link", rel: "icon", type: "image/x-icon", href: "/static/favicon.ico" },
  ],
});
```

Two rows are the same element when they share the attribute a head element is identified by: `rel`, `name`, `property`, `http-equiv`, `itemprop` or `id`, qualified by `sizes`, `media`, `type` and `hreflang`. A `link` whose `rel` names a resource, a stylesheet, a preconnect or a preload, is qualified by its `href` as well, so two preconnects are two elements. A `canonical` or an `icon` is a role and an inner route replaces it by naming it again. The host's own inferred icons sit under the layout's rows and are overridden the same way.

What is not in that list is a `script`. The document's one script is the entry, `main.ts`, which the host writes for you.

## A script is a module

Anything the page runs is a module the entry imports. A cookie banner, a theme toggle, a keyboard shortcut: each is a file under `src/` that `main.ts` imports for its effect and the build compiles it with everything else.

```ts
import { boot, enableNavigation } from "@snapfire/fsr-client";
import { registerIslands } from "@generated/islands.js";

import "./consent";
import "./theme";

registerIslands();
boot();
enableNavigation();
```

A third-party library arrives the way React did in chapter 301: its ESM build under `vendor/`, an entry in the import map and `fsr types` for its declarations. Take the ESM build. A UMD file loads as a side-effect import and assigns a global, but exports nothing, so the module that imports it has no binding to type or to call. The vendored file and the fetched declarations come from different places unless the same registry version feeds both, so read the version in each before you trust the types.

A library that has to be a `<script src>` because its vendor says so is still not a head row. It is a script element the module creates when the page needs it, which is the next section.

## Consent decides when a tag loads

The usual pattern for consent-gated analytics is to write the tag with `type="text/plain"` and a category attribute, so the banner library rewrites it into a real script once the category is accepted. That pattern exists because the tag was HTML and something had to defuse it. Without a tag there is nothing to defuse: the banner's own callbacks say when a category is accepted and the module loads the vendor from there.

Keep the banner and each vendor apart. One module orchestrates consent; one module per vendor knows how that vendor is loaded.

```ts
// src/consent.ts
import { acceptedCategory, run } from "vanilla-cookieconsent";

import { loadAnalytics } from "./analytics";

function onConsent() {
  if (acceptedCategory("analytics")) {
    loadAnalytics();
  }
}

run({ onConsent, onChange: onConsent, categories: { necessary: { enabled: true, readOnly: true }, analytics: {} }, language: { /* ... */ } });
```

```ts
// src/analytics.ts
import { get } from "@snapfire/fsr-client/store";

import { analytics } from "./store";

declare global {
  interface Window {
    dataLayer?: unknown[];
  }
}

export function loadAnalytics() {
  const id = get(analytics);
  if (!id || window.dataLayer) {
    return;
  }
  const dataLayer: unknown[] = [];
  window.dataLayer = dataLayer;
  // gtag.js reads only `Arguments` entries off the queue and skips arrays.
  function gtag(..._: unknown[]) {
    dataLayer.push(arguments);
  }
  gtag("js", new Date());
  gtag("config", id);
  const script = document.createElement("script");
  script.async = true;
  script.src = `https://www.googletagmanager.com/gtag/js?id=${id}`;
  document.head.append(script);
}
```

The shim deserves its comment. Google's snippet pushes `arguments` and gtag.js checks for exactly that; a rest parameter is an array and an array on the queue is silently ignored, so the page view never goes out and nothing says so. `onChange` runs `onConsent` again so a visitor who accepts later is counted from then on; the `dataLayer` guard makes the second call a no-op.

## The id differs per deployment

The one thing in that module the deployment owns is the id and the one place a deployment's values live is `[public]` in the configuration, chapter 200. Declare it in `app.toml` with the value development runs under, which is also what types it, then let the region's overlay set the real one:

```toml
# config/app.toml
[public]
analytics_id = ""
```

```toml
# config/app.sfo1.toml
[public]
analytics_id = "G-65TR8XV7YE"
```

A loader reads it as `ctx.config.analytics_id`, typed `string` because the declaration was a string. The root layout's `store` seeds the browser with it under a key the module reads:

```ts
// routes/layout.loader.ts
export async function load({ config }: Ctx) {
  return { analyticsId: config.analytics_id };
}

export const store = ({ data }: { data: { analyticsId: string } }) => ({ "site/analytics": data.analyticsId });
```

```ts
// src/store.ts
import { key } from "@snapfire/fsr-client/store";

export const analytics = key<string>("site/analytics");
```

The empty string in development is the whole switch: `loadAnalytics` returns before it touches the page and nothing about the banner or the module changes between a laptop and production. A `[public]` value reaches the browser, which is what the section is named for. A key that must stay on the server is not a `[public]` value.

## The policy that decides whether any of it runs

A Content-Security-Policy governs every script on the page, so a tag that loads fine without one stops the moment there is one. `[document.csp]` is that policy, written as directives and their sources rather than a string:

```toml
[document.csp]
default-src = ["'self'"]
script-src = ["'self'", "https://www.googletagmanager.com"]
img-src = ["'self'", "data:", "https://*.google-analytics.com"]
connect-src = ["'self'", "https://*.google-analytics.com"]
frame-src = ["https://www.youtube.com"]
object-src = ["'none'"]
```

The host merges in the sources only it knows. The inline import map's hash goes into `script-src`, because an import map has to be inline and the merged one matches no file on disk. Under `dev` a nonce for the refresh script goes in beside it, since that script carries the bundle id it was rendered against and has no stable hash. Nothing in the policy names either. A development host enforces what a production one does, which is where you want to find a missing origin.

Two things are worth knowing before writing one.

A hash in `script-src` makes the browser ignore `'unsafe-inline'` in that same directive. The host always adds the import map's hash, so a policy written to keep inline third-party tags working loses every one of them the moment it names `script-src` at all. `fsr doctor` reports that pair.

`'strict-dynamic'` is the usual answer for a tag manager, because it trusts whatever a trusted script loads through `document.createElement` so the allowlist stops mattering. It does not work here: it also makes the browser ignore `'self'` and every host in that directive. The entry module is a `<script src>` in the markup carrying no hash or nonce, so the page loads nothing at all. `fsr doctor` reports that too.

An allowlist covers analytics, whose origins are stable. It cannot cover an ad network, which injects scripts from origins that change per impression and often still uses inline script and `document.write`. For that, write the policy into `[document.csp_report_only]` first, which is the same shape sent as `Content-Security-Policy-Report-Only`. A browser reports against it and enforces nothing, so a deployment learns what would break before anything does. Both keys may be set at once.

## What the build can check

Every part of the arrangement is something the build reads. The head rows are data the report lists and a test asserts on. The vendor is a committed file with a recorded version. The id is a typed field, so a misspelt read is a build error and `fsr doctor` reports a loader reading a key no `[public]` declares. The consent decision is a function call in one module. The policy is a table `fsr doctor` reads, so the two traps above are caught before a deployment ships rather than by a blank page.

## A script that reads the markup

A tag reads events. A library that reads the markup, one that wires attributes it finds when it processes a node, has one more thing to know: the navigator writes markup after the document loaded. It dispatches `sf:navigate` on `document` once a payload's eager wave is applied and `sf:fill` for every deferred segment it fills, so such a library processes the document again on both. Chapter 105 does this for htmx, in both directions.

## The lab

In the storefront, add `[public] analytics_id = ""` to `config/app.toml` and read it in `routes/layout.loader.ts` as above. Boot the host: the report gains a `public` row with the empty value and the page's store seed carries `"site/analytics": ""`. Set it in `config/local.toml` and boot again: the seed carries the value and nothing else changed. Now misspell the key in the loader, `config.analytic_id`, then run `fsr build`: the typecheck refuses it, since `Config` has no such field. Put the misspelling in the overlay instead and run `fsr doctor`: the `ctx.config` check names the key the loader reads that no `[public]` declares.
