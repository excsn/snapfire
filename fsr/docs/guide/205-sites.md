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

That row and the site's own `[site]` block are two halves of one link, so `fsr` writes both rather than leaving two files to keep in step. `fsr sites link <shell> <site> --at /billing` writes them, refusing a shell that is itself a site, a site that mounts sites, a name the table already holds and a site whose `[site]` names somewhere else; `fsr sites unlink <shell> billing` takes them back out, or leaves the site's own block with `--keep-site`. `fsr sites list <shell>` reports the table resolved, each row with the prefix its artifact claims, its version, its hash and a note when it does not hold, then every version the cache holds.

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

## A site owns its own backends

A site declares `[clients]` in its own configuration and puts the contract beside it in `app/clients/`, exactly as a standalone application does. Nothing about the mount changes that: at mount the host runs the same client build over the site's configuration as over the shell's, so a `grpc` client gets its own `GrpcTransport` against its own `base_url`, an OpenAPI client its own HTTP transport, and a `transport = "mock"` client the JSON file beside it.

```toml
[clients.ledger]
transport = "grpc"
base_url = "http://ledger.internal:50051"
document = "clients/ledger.proto"
```

In the loader it is `services.ledger`, because the site's build lowered its plan against its own contract. In the host's table it is `billing:ledger`, since a site's client names carry the same `<name>:` prefix as its route ids. So the shell and a site can each have a `ledger`, pointing at different backends with different contracts, and neither can see the other's. The merge is checked rather than assumed: a genuine collision fails the mount and names the site.

Credentials are scoped the same way. A `bearer` key on a site's client installs an interceptor for that client alone, so a site's token never rides on the shell's calls or another site's.

The exception is a host built with its services supplied directly rather than from configuration. That override replaces every client the process would have built, the sites' included, which is what a test harness wants and a surprise anywhere else.

## What the shell wins

The shell owns the document, so where the two configurations disagree the shell's answer stands. Its import map overrides the site's on any shared specifier, which is what keeps one React in the page. The site's `[session]`, `[auth]` and `[cache]` are dropped, along with its `not-found.tsx`, because the shell already answers those for the whole document. A static root outside the site's own prefix is dropped rather than served, so a site cannot claim `/static/js/vendor` and shadow the shell's. Every one of those is listed under `ignored` in the boot report rather than happening quietly.

Store keys are the exception, and the only place a site and the shell can genuinely collide: nothing namespaces them, the map is flat and shared. A site prefixes its own by hand, `billing/draft` rather than `draft`, the way the shell's own keys already are.

## What a version is

An artifact is a deploy tree: the files the host reads and nothing more, laid out the way chapter 303 describes. The configuration goes under `config/`. The plan, the contracts, the service documents and the message catalogs go under `app/`. Every static root goes under `serve/` at the route it answers. Routes, sources, types and a site's own markdown are build inputs and stay out.

Two things follow from the destinations being derived rather than copied from the project. A site whose static root points at a shared build outside its own directory still packs, because where that directory sat is not what the artifact records. And a hash taken in a working tree is the hash of what a release copied out of it, because laying a tree out again yields the same tree, so a pin made in development holds against the deployed directory.

`fsr sites hash <site>` prints that hash with the parts it covers; `--files` adds every file and digest. A part is a path in the artifact rather than in the project. `fsr sites pin <shell> [<name>]` writes it into the shell's table instead of leaving it to be copied between two files, replacing whatever the mount pinned before and reporting the move. Only a `name@version` artifact is pinned: a mount naming a path is a linked working tree that changes on every build, so a pin there would be stale by the next one. `fsr sites pack <site> --version 1.4.2` writes the artifact as a gzipped tar carrying a manifest of every file with its size and sha256. Packing the same tree twice writes the same bytes, so two builders can be compared. The hash is over that listing rather than over the bytes, so a manifest alone yields it and a pin is checked before anything is downloaded.

## A deploy is a pointer moved

`[sites] root` is a cache: `<root>/<name>/<version>`, which is where `artifact = "billing@1.4.2"` already resolves. `fsr sites install <shell> billing-1.4.2.tar.gz --keep 3` unpacks into a dot-prefixed staging directory beside the destination, verifies every file against the manifest and the listing against the hash, and only then renames it into place. A fetch that dies leaves nothing a mount can see; one that arrives wrong leaves the running version serving and names the file that disagreed. `--keep` sweeps older versions and never the one in use, so a rollback is offline. It then writes the hash of what it placed into the mount that names that version, because install is the one moment when computing a hash and meaning to ship that version are the same act; `--no-pin` leaves the table alone. A mount still pointing at the previous version is left as it is, since moving the pointer is a separate decision.

Where the bytes come from is a seam, not a fixed answer: a directory of archives and a single archive ship, and an object store, a registry or a company artifact service is one method, `fetch(package, version, into)`, with the install path around it unchanged.

Then the row moves. The shell rereads the table on `SIGHUP` and on the poll, rebuilds its tables whole and swaps them; a request in flight finishes on the old ones. A pinned hash refuses bytes the table did not mean. The pin lives in the shell rather than beside the artifact on purpose: a pin inside the thing it pins is replaced by whoever replaced the artifact, so it is only worth something as a statement the shell makes about the site. `GET /__fsr/sites` lists every mounted site with its version and hash, so a monitor compares the fleet against the table and resends a signal when an instance lags.

`SIGHUP` reloads everything, the shell's own configuration and plan included, which is the wrong operation for a site deploy: one team's signal would ship whatever state the shell's files are in. A sites-only reload reads the artifacts again and rebuilds against the shell as the process booted it, its configuration, plan and contracts held rather than reread, so a half-written shell plan on disk cannot reach the tables. A shell change is a restart, which is the cost of the guarantee. For an operator who cannot signal a process at all, `POST /__fsr/sites/reload` does the same and answers with the mounted rows, or a `409` and the reason when the candidate is refused, which is what a signal cannot tell you.

`fsr sites reload [<shell dir>] --host <url>` posts it, which is the same thing from a laptop on the VPN rather than a shell on the box. `--host` repeats for a fleet and the instances are asked one at a time, stopping at the first refusal: a `409` says what was published is bad, so carrying on would ship it to the rest. `--all` continues anyway, for the case where one instance is already known to be broken. With no `--host` it reads the shell's own `server.listen`, which is the same-machine case.

`fsr sites list <shell dir> --host <url>` is the comparison rather than a dump: every row of the table beside what each instance is actually serving, marked `ok`, `lags` or `absent`, exiting non-zero when any of them disagrees. That is the monitor this section describes, as a command. It exists only when the host carries the feature and the application installed a sites mounter.

## Keep the administrative routes off the internet

`/__fsr/` is the host's administrative surface and the framework does not guard it. `fsr sites list --host` and `fsr sites reload` forward credentials with `--header "Name: Value"`, which repeats. `$FSR_SITES_HEADER` carries one that should stay out of shell history. They create no credentials of their own, because there is nothing here to authenticate against: what the header is for is whatever sits in front of the host. `GET /__fsr/sites` reports every mounted site with its version and hash, and the reload route changes what the process serves. Neither authenticates, so restricting them is the deployment's job at whatever sits in front of the host, and a host published straight to the internet with no proxy exposes both. Deny the prefix and allow it only from the network the operators are on; 404 is the better answer than 403, since it does not confirm the surface is there.

```nginx
location ^~ /__fsr/ {
  allow 10.0.0.0/8;
  deny all;
  proxy_pass http://127.0.0.1:8080;
}
```

`^~` matters in nginx: a plain `location /__fsr/` loses to any regex location and has to win against the catch-all that proxies everything else. Apache wants a `<LocationMatch "^/__fsr/">` with `Require ip`, placed before the `ProxyPass` for `/` since the first match wins. Caddy wants a `handle` on `path /__fsr/*` that responds 404 to anything outside the range. HAProxy wants `acl fsr_path path_beg /__fsr/` with an `http-request deny deny_status 404`. Envoy wants the prefix on its own route with a `direct_response`, or an RBAC filter with a `url_path` permission. Traefik wants an `ipAllowList` middleware on a ``PathPrefix(`/__fsr/`)`` router declared before the catch-all. Kubernetes ingress-nginx takes the nginx block through a `server-snippet` annotation, or a separate ingress for the prefix with `whitelist-source-range`.

Two things a proxy rule does not cover. A host bound to `0.0.0.0` is reachable around the proxy, so bind it to loopback or to the interface the proxy is on. And a sidecar or another pod on the same network is not the public internet but is not an operator either, which is what the allow list is for rather than the deny.

## The lab

Build and run `portal_react_ts` as its README says, then open `/`, sign in as `alice` and click Billing. Watch the header stay while the invoices arrive, open the browser's network view and find the payload with its `E` row, then the site's `main.js` loaded from `/billing/static/js/app/`. Click Overdue: the site's guard let you through on the portal's sign-in. Now stop the portal, edit the billing site's overdue page, rebuild it and `kill -HUP` the portal: `GET /__fsr/sites` shows a new hash and the page shows your edit, with your session intact.
