# 304. Checking a deployment

The question this chapter answers: what is wrong with an application that starts, serves and answers, but does not do what its configuration says it does?

**For:** everyone, before a deploy.

## What the host already refuses

The host is strict at boot on purpose. A declared action nothing answers, a bundle carrying a server module, a route claimed twice, a plan file it cannot read, a `[server] render` line: each of those stops the process with the reason, because a boot failure is easier to notice than a deployment that partly works.

Everything the host catches is a contradiction it can see from inside a single boot. It cannot catch a setting that is coherent, loads cleanly and still cannot do the job it was written for. A locale in the table with no catalog file loads: `t` falls back and the page renders in the wrong language. An import map naming a package nothing vendored loads: the browser asks for the file and gets a 404. A plan older than the routes it was lowered from loads cleanly, then answers the old routes.

`fsr doctor` checks for those.

```
fsr doctor app
```

It reads the configuration and the plan the way the host would, runs every check and prints what it found with what to do about it. Nothing found is exit 0. Anything found is exit 1, so a deploy script can stop on it:

```sh
fsr build app && fsr doctor app
```

`fsr bundle` runs the same checks itself and refuses on a finding, so a deploy that ends in a bundle needs no separate step:

```sh
fsr build app && fsr bundle app
```

Run the command on its own in CI, where there is a plan to check but no tree to write. `fsr bundle --no-doctor` skips the check for a caller that means to bundle anyway.

## What it checks

Each check answers from something a build already computed, so none of it needs a server, a browser or a network.

| Check | What it reports | Why it matters |
| --- | --- | --- |
| `canonical` | `[document] origin` is unset while the deployment names hosts or prerenders | Without an origin the canonical and alternate links are relative, which an audit reports and a crawler resolves against whatever host it arrived on |
| `ctx.host` | a body reads `ctx.host` while `[server] hosts` is empty | The list is the only opt-in, so an empty one means the read answers null for ever rather than the host the request carried |
| `ctx.config` | a body reads `ctx.config.<key>` while `[public]` does not declare the key | The declaration is what types the read and what an overlay sets, so a key missing from it answers null on every deployment |
| `locales` | a locale in `[locales] supported` with no catalog under `locales/` | The application says it serves that language and every message falls back |
| `stale` | the plan is missing or older than `routes/`, `src/`, `clients/` or `schemas/` | The host reads the plan and never the sources, so an unbuilt change is invisible until the next build |
| `vendor` | the import map names a package with nothing under `vendor/` to answer it | The browser asks for the file the map names, so a missing one is a page that does not mount |
| `statics` | a `[[static]]` root whose directory is not there | Every path under that route answers 404, including the client bundle when it is served that way |
| `shadow` | a `[[static]]` root whose route swallows an application route | A matched static prefix answers from the directory and returns, so the page underneath it never runs |
| `bearer` | a client carries a bearer token while `[auth]` is unset | An `[auth]` provider is the only thing that writes a token into custody, so the call goes out with no `Authorization` header |
| `cache.tags` | a call drops a cache tag no cached method names | The two sides are strings that have to agree; a typo either way leaves a write that invalidates nothing |
| `links` | a literal internal link matching no route, static root or mounted site | The plan already holds the link and the routes, so a 404 you would otherwise find only by clicking shows up without a crawler |
| `render` | a `[server] render` line | Every page renders on the server and every island in the browser, so no host accepts the setting and the boot stops until the line is taken out |
| `tree` | a file a deploy tree would carry that the project does not hold; a setting no tree can express | The host reads each of these at boot, so a tree without one starts on the machine that built it and fails on the machine it was copied to |
| `csp` | a policy naming `'unsafe-inline'` in `script-src` beside an import map; a policy naming `'strict-dynamic'` | The host adds the import map's hash to that directive; a hash makes the browser ignore `'unsafe-inline'`. `'strict-dynamic'` makes it ignore `'self'` and every host, leaving the entry module with nothing to allow it |
| `sites` | a mounted site that pins no hash, ships a part the artifact does not carry, has no plan or one older than its own routes, plus artifacts under the root no mount names | A shell serves a site it never builds, so nothing about the artifact is checked until a request asks for it |

A report names the check, the fact and the remedy:

```
canonical    `document.origin` is unset while `server.hosts` names 2 hosts, so every canonical and alternate link is relative
             set `[document] origin` to the address this deployment is reached at, `https://example.com`
doctor       1 of 14 checks found something
```

### Plan and configuration checks

Four of the checks compare two artifacts that were written separately and have to agree.

`shadow` is the one with a boot error beside it. The host refuses to start when two plans claim one route and says nothing when a static root claims one, even though the outcome is worse: a matched static prefix answers and returns rather than falling through, so the page under it is unreachable for as long as both are declared.

`bearer` and `cache.tags` are both a name that only works if two places spell it the same. A bearer client reads a token out of custody and an `[auth]` provider is the only thing that puts one there. A cache tag is dropped by whatever names it in `writes`. Only the write side of a tag is asked about, because a cached tag nothing writes is how a read-only service says it expires by its own ttl. A typo in either direction leaves a written tag nothing caches.

`links` reads the literal `href` of every anchor the build lowered and asks whether this deployment answers it, against its routes, its static roots, the prefixes of the sites it mounts and the framework's own paths. A locale prefix is stripped first, the way the host strips one. Only a literal is asked about, since the build cannot know every value a computed href takes. A site is exempt: its links reach into a shell it cannot see, so the shell it was built against need not be the one it runs in.

### Deploy tree checks

The other checks read settings. This one reads the deploy tree that `fsr bundle` is about to write, before it exists, then reports a file the tree would name and the project cannot supply.

That is a class of failure the other checks cannot see, because the files it covers are not named in any setting. The host imports each service client's document at boot and will not start without it. It reads `locales/` by name and an application whose catalogs went missing serves message keys. Neither is a path anybody writes down, so the only way to check them is to derive what the tree holds from what the host reads, which is what the bundle does anyway. Chapter 303 describes that derivation.

The check also refuses a setting no tree can express. A `[[static]]` route that climbs out of its own prefix has no place to land under `serve/`, so the layout stops rather than picking one.

The plan and the import map are left to `stale` and `vendor`, which already report them with remedies of their own.

### Site checks

A shell serves a mounted site and never builds it, so the artifact is the only thing that says what it should carry.

A mount that pins no `hash` accepts whatever sits at the path. The pin is what makes a deploy reproducible; `fsr sites hash <site dir>` prints the one to set. Only a `name@version` artifact is asked for one: a mount naming a path is a linked working tree that changes on every build, so a pin there would be stale by the next one.

A part the artifact says it ships and does not carry is hashed as absent rather than refused, so the site mounts and then answers 404 for its own assets. That happens when a site is packed without being rebuilt.

Artifacts under the sites root that no mount names are what `fsr sites install` leaves behind. They cost disk and they make it hard to tell which version is live; `--keep <n>` bounds them.

A pin the artifact does not match is reported with what to do about it. The content changed. If that was meant, `fsr sites pin` records it. If it was not, the directory is not the version the shell pinned. A mount pointing at nothing is reported too. Both are refusals to start rather than warnings; doctor reports them before the deploy and the host still refuses them at boot.

## What doctor does not do

**It never fixes anything.** Every finding here has a remedy that is a judgement: whether a locale should gain a catalog or leave the table, whether a link is wrong or a route is missing. So doctor reports the finding and leaves the choice to you.

**It never softens a boot error.** Anything the host refuses to start over still fails at boot. Doctor reports what has nowhere else to be reported; it does not downgrade failures.

**It only checks contradictions.** Every check covers a fact the application stated and then contradicted, never a matter of taste. "The locale table names `fr` and there is no `fr` catalog" is a fact. Style questions such as how long a title should be are not checked. There is no configuration file for turning checks off.

## Where it belongs

Run it in CI after the build, then in a deploy script before the bundle. It is fast and needs nothing running, so it is the last thing that reads the whole configuration before a server does.
