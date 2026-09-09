# 304. Checking a deployment

The question this chapter answers: what is wrong with an application that starts, serves and answers, but does not do what its configuration says it does?

**For:** everyone, before a deploy.

## What the host already refuses

The host is strict at boot on purpose. A declared action nothing answers, a bundle carrying a server module, a route claimed twice, a plan file it cannot read, a `server.render` that is neither `rust` nor `islands`: each of those stops the process with the reason, because a deployment that half works is worse than one that does not start.

That strictness has an edge. Everything it catches is a contradiction the host can see from inside a single boot. What it cannot see is a setting that is coherent, loads cleanly and still cannot do the job it was written for. A locale in the table with no catalog file loads: `t` falls back and the page renders in the wrong language. An import map naming a package nothing vendored loads: the browser asks for the file and gets a 404. A plan older than the routes it was lowered from loads perfectly, then answers yesterday's routes.

`fsr doctor` is that middle.

```
fsr doctor app
```

It reads the configuration and the plan the way the host would, runs every check and prints what it found with what to do about it. Nothing found is exit 0. Anything found is exit 1, so a deploy script can stop on it:

```sh
fsr build app && fsr doctor app && fsr bundle app
```

## What it checks

Each check answers from something a build already computed, so none of it needs a server, a browser or a network.

| Check | What it reports | Why it matters |
| --- | --- | --- |
| `canonical` | `[document] origin` is unset while the deployment names hosts or prerenders | Without an origin the canonical and alternate links are relative, which an audit reports and a crawler resolves against whatever host it arrived on |
| `ctx.host` | a body reads `ctx.host` while `[server] hosts` is empty | The list is the whole opt-in, so an empty one means the read answers null for ever rather than the host the request carried |
| `locales` | a locale in `[locales] supported` with no catalog under `locales/` | The application says it serves that language and every message falls back |
| `stale` | the plan is missing or older than `routes/`, `src/`, `clients/` or `schemas/` | The host reads the plan and never the sources, so an unbuilt change is invisible until the next build |
| `vendor` | the import map names a package with nothing under `vendor/` to answer it | The browser asks for the file the map names, so a missing one is a page that does not mount |
| `render` | `[server] render` is `islands` and the plan carries no island | Every page is handed to the browser to render, for no reason |
| `statics` | a `[[static]]` root whose directory is not there | Every path under that route answers 404, including the client bundle when it is served that way |
| `sites` | a mounted site that pins no hash, ships a part the artifact does not carry, has no plan or one older than its own routes, plus artifacts under the root no mount names | A shell serves a site it never builds, so nothing about the artifact is checked until a request asks for it |

A report names the check, the fact and the remedy:

```
canonical    `document.origin` is unset while `server.hosts` names 2 hosts, so every canonical and alternate link is relative
             set `[document] origin` to the address this deployment is reached at, `https://example.com`
doctor       1 of 8 checks found something
```

### What a shell owes its sites

A shell serves a mounted site and never builds it, so the artifact is the only thing that says what it should carry. Three of those findings are worth spelling out.

A mount that pins no `hash` accepts whatever sits at the path. The pin is what makes a deploy reproducible; `fsr sites hash <site dir>` prints the one to set. Only a `name@version` artifact is asked for one: a mount naming a path is a linked working tree that changes on every build, so a pin there would be stale by the next one.

A part the artifact says it ships and does not carry is hashed as absent rather than refused, so the site mounts and then answers 404 for its own assets. That happens when a site is packed without being rebuilt.

Artifacts under the sites root that no mount names are what `fsr sites install` leaves behind. They cost disk and they make it hard to tell which version is live; `--keep <n>` bounds them.

A mount the table points at with nothing there is reported too, though that one is a refusal to start rather than a warning. Doctor says it before the deploy rather than instead of it.

## What it will not do

**It never fixes anything.** Every finding here has a remedy that is a judgement: whether a locale should gain a catalog or leave the table, whether an island is missing or the render mode is wrong. A flag that picked one would be wrong half the time and silent about it.

**It never softens a boot error.** A condition the host refuses to start over stays a condition the host refuses to start over. Doctor exists to surface what has nowhere else to go, not to move failures somewhere quieter.

**It has no opinions.** Every check is a fact the application already stated and then contradicted, never a matter of taste. "The locale table names `fr` and there is no `fr` catalog" is a fact. How long a title should be is not. There is no configuration file for turning checks off either, because there is nothing here worth turning off.

## Where it belongs

In CI after the build, then in a deploy script before the bundle. It is fast and needs nothing running, so it is the last thing that reads the whole configuration before a server does.
