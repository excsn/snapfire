# 205. Sites

The question this chapter answers: how does a team own its part of the product, deploy it on its own schedule and still share the header, the sign-in and the navigation with everyone else?

**For:** everyone. App developers write a site; platform developers run the shell that mounts it.

## Declaring a site

Add a `[site]` section to the configuration beside an application and it becomes a site. The build prefixes every id it emits with the name and puts every route under the prefix, so two sites can hold the same files.

```toml
[site]
name = "billing"
at = "/billing"
shell = "../portal/app/generated/shell.json"
```

Nothing in the site's TypeScript changes. Routes are still `routes/invoice/[id]/page.tsx`, a loader still calls `services.ledger`, a page still calls `actions.invoice.pay`. The paths are literal, as they are everywhere in fsr: a link is `/billing/invoice/1` and the middleware compares against `/billing/overdue`. Only the plan file and the browser bundle spell the prefix, `billing:routes/invoice/[id]/page.tsx#default` and `billing:invoice.pay` and the host reads them that way.

## Running a site on its own

A site runs alone with `fsr dev` or `cargo run`, under its own prefix with its own layout as the page, so a team develops and tests it without the shell. Alone it keeps what a mount drops: its own `[session]`, `[auth]`, `[cache]` and `not-found.tsx`. It has no sign-in but its own `[auth]`, so a guard that redirects to the shell's login lands on a page that is not there.

Its frameworks still come from the shell. The site's import map names React at the shell's URL, so when the `generated/shell.json` that `[site] shell` names sits in the shell's `app/` beside a `vendor/` directory, the host serves that directory at `/static/js/vendor` and the report lists it under `inferred`. That root is only for the site running alone. A mount never reads it and a deploy tree never carries it, so the site's artifact and its hash are the same either way.

Running alone is for development and tests. A site is deployed by mounting it: a deploy tree has no shell beside it, so a site deployed by itself has no framework to import.

## Mounting sites in the shell

The shell is an application the platform team owns: the root layout, the sign-in, the locales, the vendor tree. Its configuration names the sites it mounts.

```toml
[sites]
root = "/srv/sites"
poll = "30s"

[sites.billing]
artifact = "billing@1.4.2"
hash = "3a098783bbb3ebc5"
```

That row and the site's own `[site]` block describe the same link, so `fsr` writes both rather than leaving two files to keep in step. `fsr sites link <shell> <site> --at /billing` writes them, refusing a shell that is itself a site, a site that mounts sites, a name the table already holds and a site whose `[site]` names somewhere else; `fsr sites unlink <shell> billing` takes them back out or leaves the site's own block with `--keep-site`. `fsr sites list <shell>` reports the table resolved, each row with the prefix its artifact claims, its version, its hash and a note when it does not hold, then every version the cache holds.

At boot the host reads each artifact, the directory a site's build leaves behind, checks that it is the site it claims to be, refuses one carrying engine-owned rows or a leaked server module, nests its routes under the shell's root layout and adds its rows to the shell's tables. The shell and its sites share one document, session and navigation: a click from the portal's directory into `/billing` is a payload navigation that keeps the header's island and imports the site's islands on the way, from an `E` row the payload carries. The site's entry calls `enableNavigation` too, as every entry does. A second call on a document the navigator is already wired to leaves the navigation that loaded it in place, so the click after it patches as well.

The shell's middleware runs first on every path, with `request.site` naming the site a path belongs to. The site's runs second on the same path and may only narrow what the shell allowed. The shell's sign-in reaches the site's loaders and middleware as `identity`, so a site guards a route without ever seeing a password.

## What the shell and a site share

Two typed things pass between them and neither is a runtime call. The shell's build writes `generated/shell.json`: every store key its loaders seed, typed as the browser reads it, the import map it serves and the exact version of each framework it vendors. A site names that file with `[site] shell` and its build writes `generated/shell.d.ts`, so a site reads the shell's store with the right type and imports React from the shell's URL rather than its own copy.

```ts
import { key } from "@snapfire/fsr-client/store";
import type { ShellStore } from "@generated/shell";

export const who = key<ShellStore["portal/who"]>("portal/who");
```

The other direction is the site's own contract: its clients, its cache tags, its types, all prefixed, merged into the shell's registry without a collision.

## A site owns its own backends

A site declares `[clients]` in its own configuration and puts the contract beside it in `app/clients/`, exactly as a standalone application does. Nothing about the mount changes that: at mount the host runs the same client build over the site's configuration as over the shell's, so a `grpc` client gets its own `GrpcTransport` against its own `base_url`, an OpenAPI client its own HTTP transport and a `transport = "mock"` client the JSON file beside it.

```toml
[clients.ledger]
transport = "grpc"
base_url = "http://ledger.internal:50051"
document = "clients/ledger.proto"
```

In the loader it is `services.ledger`, because the site's build lowered its plan against its own contract. In the host's table it is `billing:ledger`, since a site's client names carry the same `<name>:` prefix as its route ids. So the shell and a site can each have a `ledger`, pointing at different backends with different contracts and neither can see the other's. The merge is checked: a genuine collision fails the mount and names the site.

Credentials are scoped the same way. A `bearer` key on a site's client installs an interceptor for that client alone, so a site's token never rides on the shell's calls or another site's.

The exception is a host built with its services supplied directly rather than from configuration. That override replaces every client the process would have built, the sites' included, so it is meant for a test harness and nowhere else.

## Where the shell overrides a site

The shell owns the document, so where the two configurations disagree the shell's answer stands. Its import map overrides the site's on any shared specifier, which is what keeps one React in the page. The site's `[session]`, `[auth]` and `[cache]` are dropped, along with its `not-found.tsx`, because the shell already answers those for the whole document. A static root outside the site's own prefix is dropped rather than served, so a site cannot claim `/static/js/vendor` and shadow the shell's. The roots the host infers are already under the prefix for a site, its `vendor/` tree along with its stylesheets and icons, so a site that vendors a package of its own is served it at `/billing/static/js/vendor`. Every one of those is listed under `ignored` in the boot report rather than happening quietly.

A mounted site therefore vendors no framework of its own. Its build reads the React version it renders under out of the contract, so there is no second copy to keep in step with the shell's. A site that does vendor one must vendor the version the shell serves. The site's map still names `react` at the shell's URL. The build refuses a framework specifier pointed anywhere else, so the entry cannot drift from what the shell answers.

Store keys are the exception and the only place a site and the shell can genuinely collide: nothing namespaces them, the map is flat and shared. A site prefixes its own by hand, `billing/draft` rather than `draft`, the way the shell's own keys already are.

## What a version is

An artifact is a deploy tree: the files the host reads and nothing more, laid out the way chapter 303 describes. The configuration goes under `config/`. The plan, the contracts, the service documents and the message catalogs go under `app/`. Every static root goes under `serve/` at the route it answers. Routes, sources, types and a site's own markdown are build inputs and stay out.

The destinations are derived from the configuration rather than copied from the project. So a site whose static root points at a shared build outside its own directory still packs, because the artifact does not record where that directory sat. A hash taken in a working tree is the hash of what a release copied out of it, because laying a tree out again yields the same tree, so a pin made in development holds against the deployed directory.

`fsr sites hash <site>` prints that hash with the parts it covers; `--files` adds every file and digest. A part is a path in the artifact rather than in the project. `fsr sites pin <shell> [<name>]` writes it into the shell's table instead of leaving it to be copied between two files, replacing whatever the mount pinned before and reporting the move. Only a `name@version` artifact is pinned: a mount naming a path is a linked working tree that changes on every build, so a pin there would be stale by the next one. `fsr sites pack <site> --version 1.4.2` writes the artifact as a gzipped tar carrying a manifest of every file with its size and sha256. Packing the same tree twice writes the same bytes, so two builders can be compared. The hash is over that listing rather than over the bytes, so a manifest alone yields it and a pin is checked before anything is downloaded.

## How a site reaches the shell

A `[sites.<name>]` row names its artifact one of two ways and a version gets into the cache one of two ways, which gives three shapes. The portal example mounts one site of each.

| Row | Where the artifact comes from | For |
| --- | --- | --- |
| `artifact = "sites/billing"` | The site's working tree, read in place. Reported as version `path` and never pinned, since it changes on every build. | Developing the shell and its sites together in one repository. |
| `artifact = "billing@1.4.2"`, installed | `<root>/billing/1.4.2`, placed by `fsr sites install` from an archive before the row moves. | An operator who ships each release by hand or from a job. |
| `artifact = "billing@1.4.2"`, fetched | The same directory, filled by the shell from `[sites] store` when the cache lacks the version. | A release pipeline that publishes archives to a registry: moving the row is the deploy. |

In all three the site runs inside the shell's process. The shell reads the site's plan, runs its loaders and actions and renders its pages under its own root layout. That is how they share one document, session and navigation. A site running in a process of its own with the shell proxying to it is not something FSR does: the shell would have to forward the session and identity on every request, splice another server's HTML and payload under its layout and decide what a page shows when that server is down. A site that needs a backend of its own gets one through `[clients]`, described above. Its pages still render in the shell.

## Deploying a site version

`[sites] root` is a cache: `<root>/<name>/<version>`, which is where `artifact = "billing@1.4.2"` already resolves. `fsr sites install <shell> billing-1.4.2.tar.gz --keep 3` unpacks into a dot-prefixed staging directory beside the destination, verifies every file against the manifest and the listing against the hash and only then renames it into place. A fetch that dies leaves nothing a mount can see; one that arrives wrong leaves the running version serving and names the file that disagreed. `--keep` sweeps older versions and never the one in use, so a rollback is offline. It then writes the hash of what it placed into the mount that names that version, because installing a version is when you mean to ship it; `--no-pin` leaves the table alone. A mount still pointing at the previous version is left as it is, since moving the pointer is a separate decision.

The shell can also fetch a version itself. `[sites] store` names where a version the cache lacks comes from: a directory of `<name>-<version>.tar.gz` archives against the project root or an `http://` or `https://` URL answering `GET <url>/<name>-<version>.tar.gz`. On boot and on every reload the shell fetches each `name@version` row the cache does not hold, stages it, verifies it the way `install` does and renames it into place before mounting anything. A fetch that fails refuses the mount and names the row, so a reload leaves the running version serving.

```toml
[sites]
root = "/srv/sites"
store = "https://artifacts.example.com/sites"

[sites.billing]
artifact = "billing@1.4.2"
hash = "3a098783bbb3ebc5"
```

`FSR_SITES_STORE_HEADER="Authorization: Bearer <token>"` puts a header on every fetch, which keeps the token out of the table. Any server answering those names is a store: nginx over a directory, a bucket, an Artifactory generic repository. A store that needs more than a GET, a registry with its own protocol or a company artifact service, is a type implementing `snapfire_fsr_sites::Store`, one method, `fetch(package, version, into)`. A shell with Rust of its own hands it to `mount_all_with` and `mountable_with`. `fsr serve` has the directory and HTTP stores only.

Then move the row. The shell rereads the table on `SIGHUP` and on the poll, rebuilds all its tables and swaps them; a request in flight finishes on the old ones. A pinned hash refuses bytes the table did not mean. The pin lives in the shell rather than beside the artifact on purpose: a pin stored inside the artifact would be replaced along with the artifact, so only a pin the shell holds can catch a wrong one. `GET /__fsr/sites` lists every mounted site with its version and hash, so a monitor compares the fleet against the table and resends a signal when an instance lags.

`SIGHUP` reloads everything, the shell's own configuration and plan included, which is the wrong operation for a site deploy: one team's signal would ship whatever state the shell's files are in. A sites-only reload reads the artifacts again and rebuilds against the shell as the process booted it, its configuration, plan and contracts held rather than reread, so a half-written shell plan on disk cannot reach the tables. A shell change needs a restart. For an operator who cannot signal a process at all, `POST /__fsr/sites/reload` does the same and answers with the mounted rows or a `409` and the reason when the candidate is refused. A signal gives no such answer.

`fsr sites reload [<shell dir>] --host <url>` posts it, which is the same thing from a laptop on the VPN rather than a shell on the box. `--host` repeats for a fleet and the instances are asked one at a time, stopping at the first refusal: a `409` says what was published is bad, so carrying on would ship it to the rest. `--all` continues anyway, for the case where one instance is already known to be broken. With no `--host` it reads the shell's own `server.listen`, which is the same-machine case.

`fsr sites list <shell dir> --host <url>` compares the table against each instance: every row of the table beside what each instance is actually serving, marked `ok`, `lags` or `absent`, exiting non-zero when any of them disagrees. It does the monitor's job described above as a command. It exists only when the host carries the feature and the application installed a sites mounter.

## Protecting the administrative routes

`/__fsr/` is the host's administrative surface and the framework does not guard it. `fsr sites list --host` and `fsr sites reload` forward credentials with `--header "Name: Value"`, which repeats. `$FSR_SITES_HEADER` carries one that should stay out of shell history. They create no credentials of their own, because the host has nothing to authenticate against; the header is for whatever sits in front of the host. `GET /__fsr/sites` reports every mounted site with its version and hash and the reload route changes what the process serves. Neither authenticates, so restricting them is the deployment's job at whatever sits in front of the host and a host published straight to the internet with no proxy exposes both. Deny the prefix and allow it only from the network the operators are on; 404 is the better answer than 403, since it does not confirm the surface is there.

```nginx
location ^~ /__fsr/ {
  allow 10.0.0.0/8;
  deny all;
  proxy_pass http://127.0.0.1:8080;
}
```

`^~` matters in nginx: a plain `location /__fsr/` loses to any regex location and has to win against the catch-all that proxies everything else. Apache wants a `<LocationMatch "^/__fsr/">` with `Require ip`, placed before the `ProxyPass` for `/` since the first match wins. Caddy wants a `handle` on `path /__fsr/*` that responds 404 to anything outside the range. HAProxy wants `acl fsr_path path_beg /__fsr/` with an `http-request deny deny_status 404`. Envoy wants the prefix on its own route with a `direct_response` or an RBAC filter with a `url_path` permission. Traefik wants an `ipAllowList` middleware on a ``PathPrefix(`/__fsr/`)`` router declared before the catch-all. Kubernetes ingress-nginx takes the nginx block through a `server-snippet` annotation or a separate ingress for the prefix with `whitelist-source-range`.

A proxy rule does not cover a host reached around the proxy or a neighbour on the same network. A host bound to `0.0.0.0` is reachable around the proxy, so bind it to loopback or to the interface the proxy is on. A sidecar or another pod on the same network is neither the public internet nor an operator, so the allow list is what keeps it out, not the deny.

## The lab

Build and run `portal_react_ts` as its README says, then open `/`, sign in as `alice` and click Billing. Watch the header stay while the invoices arrive, open the browser's network view and find the payload with its `E` row, then the site's `main.js` loaded from `/billing/static/js/app/`. Click Overdue: the site's guard let you through on the portal's sign-in. Click Blog, Status and then Billing again: each hop is a payload navigation and the header's team count survives all of them. Now edit the blog's index page, run `fsr build sites/blog/app` and `kill -HUP` the portal. The blog is a linked tree, so `GET /__fsr/sites` shows a new hash and the page shows your edit, with your session intact. For a release rather than an edit, pack status as `1.1.0` into the store and move `[sites.status] artifact` to `status@1.1.0`: within the poll the portal fetches it and `/__fsr/sites` reports the new version.
