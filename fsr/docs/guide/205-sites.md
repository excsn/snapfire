# 205. Sites: one product, many teams

The question this chapter answers: how does a team own its part of the product, deploy it on its own schedule and still share the header, the sign-in and the navigation with everyone else?

**For:** everyone. App developers write a site; platform developers run the shell that mounts it.

## A site is an application with a name

Add a `[site]` section to the configuration beside an application and it becomes a site. The build prefixes every id it emits with the name and puts every route under the prefix, so two sites can hold the same files.

```toml
[site]
name = "billing"
at = "/billing"
shell = "../portal/app/generated/shell.json"
```

Nothing in the site's TypeScript changes. Routes are still `routes/invoice/[id]/page.tsx`, a loader still calls `services.ledger`, a page still calls `actions.invoice.pay`. The paths are literal, as they are everywhere in fsr: a link is `/billing/invoice/1` and the middleware compares against `/billing/overdue`. Only the plan file and the browser bundle spell the prefix, `billing:routes/invoice/[id]/page.tsx#default` and `billing:invoice.pay`, and the host reads them that way.

A site runs alone with `fsr dev` or `cargo run`, its own layout as the page, so a team develops and tests it without the shell.

## The shell mounts it

The shell is an application the platform team owns: the root layout, the sign-in, the locales, the vendor tree. Its configuration names the sites it mounts.

```toml
[sites]
root = "/srv/sites"
poll = "30s"

[sites.billing]
artifact = "billing@1.4.2"
hash = "3a098783bbb3ebc5"
```

At boot the host reads each artifact, the directory a site's build leaves behind, checks that it is the site it claims to be, refuses one carrying engine-owned rows or a leaked server module, nests its routes under the shell's root layout and adds its rows to the shell's tables. One document, one session, one navigation: a click from the portal's directory into `/billing` is a payload navigation that keeps the header's island and imports the site's islands on the way, from an `E` row the payload carries.

The shell's middleware runs first on every path, with `request.site` naming the site a path belongs to. The site's runs second on the same path and may only narrow what the shell allowed. The shell's sign-in reaches the site's loaders and middleware as `identity`, so a site guards a route without ever seeing a password.

## What crosses the seam

Two things, both typed, neither a runtime call. The shell's build writes `generated/shell.json`: every store key its loaders seed, typed as the browser reads it, plus the import map it serves. A site names that file with `[site] shell` and its build writes `generated/shell.d.ts`, so a site reads the shell's store with the right type and imports React from the shell's URL rather than its own copy.

```ts
import { key } from "@snapfire/fsr-client/store";
import type { ShellStore } from "@generated/shell";

export const who = key<ShellStore["portal/who"]>("portal/who");
```

The other direction is the site's own contract: its clients, its cache tags, its types, all prefixed, merged into the shell's registry without a collision.

## A deploy is a pointer moved

A team deploys by laying a new version under the root and moving the row in the table. The shell rereads the table on `SIGHUP` and on the poll, rebuilds its tables whole and swaps them; a request in flight finishes on the old ones. A pinned hash refuses bytes the table did not mean. `GET /__fsr/sites` lists every mounted site with its version and hash, so a monitor compares the fleet against the table and resends a signal when an instance lags.

## The lab

Build and run `portal_react_ts` as its README says, then open `/`, sign in as `alice` and click Billing. Watch the header stay while the invoices arrive, open the browser's network view and find the payload with its `E` row, then the site's `main.js` loaded from `/billing/static/js/app/`. Click Overdue: the site's guard let you through on the portal's sign-in. Now stop the portal, edit the billing site's overdue page, rebuild it and `kill -HUP` the portal: `GET /__fsr/sites` shows a new hash and the page shows your edit, with your session intact.
