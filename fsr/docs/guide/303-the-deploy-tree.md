# 303. The deploy tree

The question this chapter answers: what do you copy to a server, and how does what it serves there stay the same as what it served under `fsr dev`?

**For:** everyone.

## One directory is servable and the rest is not

An application's directory is mostly things a server must never hand out. `routes/` and `src/` are the TypeScript the application is written in. `types/` is declarations for the editor. `tsconfig.json` and `tsconfig.build.json` are compiler input. `.fsr-bundle/` is the overlay chapter 300 describes. None of it is reachable in development either, because the host serves the static roots and nothing else, but a deployment that copies the app directory wholesale puts every one of those files under a web server's root and hopes the routing hides them.

So a deploy tree has one directory a web server is pointed at, `serve/`, and everything else sits outside it:

```
dist/
  snapfire_www             the binary
  fibre_logging.yaml
  config/
    app.toml               and the environment files beside it
    bundle.toml            written by the bundle, the paths that moved
  app/
    generated/             plan.sexp and contracts/, read at boot
    clients/               the service documents, imported at boot
    locales/               the message catalogs
    importmap.json
  serve/
    static/js/app/         the bundle
    static/js/vendor/      the vendored packages
    static/icons/
    static/css/
```

The process reads `config/` and `app/` from its working directory. The web server reads `serve/` and can reach nothing above it. A `.tsx` file cannot be served by accident, because it is not there.

## Where a file lands is decided by what it is

A tree is the same shape whatever shape the project had. Configuration goes under `config/`, everything the host reads at boot goes under `app/` and a static root goes under `serve/` at the route it answers. None of those destinations is the path the file had in the project.

That matters because a project's paths are a developer's convenience and a tree's paths are a contract. Nine of the examples in this repository point a static root at one shared client build, `dir = "../../../client/dist"`, which is the sensible thing to do in a directory of examples and a path that means nothing once the files are copied to a server. Under the tree's rule that root is `serve/static/js/fsr`, because that is the route it answers and where it was checked out no longer signifies.

The same rule is what keeps a bundle inside its own output directory. A destination is built from a route or a fixed name, never joined together out of a configured path, so no setting can place a file above `--out`.

## `config/bundle.toml` names what moved

Moving a file means the configuration no longer describes where it is, so the bundle writes the difference into `config/bundle.toml` and the host loads it last, after `app.toml` and after every environment overlay. Chapter 200 lists that order.

```toml
[app]
dir = "app"

[server]
contracts = "generated/contracts"
plan = "generated/plan.sexp"

[document]
entry = "/static/js/app/src/main.js"
import_map = "importmap.json"
styles = ["/static/css/handbook.css"]

[[static]]
dir = "../serve/static/js/app"
route = "/static/js/app"
```

It carries more than paths. Chapter 200 describes what the host infers from an application directory: `dist/` becomes a static root at the public path its build facts name, `icons/` adds the favicon links, `styles/` adds the stylesheets. A tree has no `icons/` or `styles/` of its own, because those are static roots and they live under `serve/` now, so inference would find nothing and the head would come out empty. The bundle writes down what inference concluded rather than leaving the tree to conclude it again from a directory layout that is no longer the one inference was written for.

A tree therefore serves what the build saw, not what a second reading of a different directory produces. It also means laying a tree out again yields the same tree, byte for byte, which is what lets a site artifact be verified against the hash of the bundle that produced it.

## `fsr bundle` writes it

`fsr bundle <app> [--out <dir>]` produces that tree. It defaults to `dist/` beside the project.

It runs the checks chapter 304 describes before it writes anything, so a finding stops it. A bundle is a thing about to be shipped, so the moment to notice that the canonical link is relative or that a mounted site pins nothing is before the tree exists rather than after a server is serving it. `--no-doctor` bundles anyway, for the case where the finding is understood and the tree is wanted regardless.

```
$ fsr bundle app --out dist
serve/static/js/fsr              serves /static/js/fsr
serve/static/js/app              serves /static/js/app
serve/static/js/vendor           serves /static/js/vendor
serve/static/icons               serves /static/icons
serve/static/css                 serves /static/css
app/generated/contracts          read by the host
app/generated/plan.sexp          read by the host
app/importmap.json               read by the host
config                           read by the host
config/bundle.toml               read by the host

110 files, 639502 bytes under dist
place beside it: the binary, the logging configuration
```

The routes on the left are not a list the command carries. They are the host's own static roots, read from the configuration the same way the host reads them at boot: the `[[static]]` entries the file declares, plus the four the host infers, which chapter 200 covers, `dist/` at the public path from `dist/.snapfire-build.json`, `vendor/`, `icons/` and `styles/`. So the URLs a server answers from disk are the URLs the host answered in development, and adding a `[[static]]` entry changes the deploy without anyone editing a build script. A hand written copy list is the same information written twice, and the second copy is the one that goes stale.

The list on the right is derived the same way, from what the host reads at boot rather than from a directory the bundle sweeps. `app/clients/` is there when the application declares a service, because the host imports each client's document at boot and refuses to start without it. `app/locales/` is there when the application has message catalogs, because the host reads that directory by name and an application whose catalogs did not ship serves message keys instead of messages. Neither is named in any configuration setting, which is exactly why the list has to come from the host's own reads.

`server.prerender`, when configured, is copied beside the plan: those documents are read by the host and answered from the file, not served off disk.

A static root is copied whole, so whatever the directory holds is served. A vendored package that ships its `.d.ts` beside its `.js` puts those declarations under the route too. The browser never asks for them and the host would have served them in development just the same, but a directory you point a route at is a directory you have published.

## What it deliberately does not hold

The binary and the logging configuration. Those are the two things that differ per deployment and per tool: which binary and which logging file becomes `fibre_logging.yaml`. The command names them rather than guessing, and the build script that calls it places them:

```sh
fsr bundle app --out dist

cp target/release/snapfire_www dist/
cp fibre_logging.production.yaml dist/fibre_logging.yaml
```

`config/` does ship, whole. A tree is deployed under a `RELEASE_ENV` the bundle did not necessarily run under, so it carries every environment file in the directory rather than the ones this run happened to load.

## Pointing a web server at it

nginx serves `serve/` off disk and proxies everything else to the process:

```nginx
location ~ ^/static {
  root /excsn/apps/snapfire/www/current/serve/;
  try_files $uri $uri/ =404;
}

location / {
  proxy_pass http://127.0.0.1:11110;
  proxy_set_header Host $host;
}
```

The process still knows those routes, so it answers them when nothing is in front of it, which is what `fsr serve` does and what a container without a web server does. The two agree because both read the same configuration.

## The lab

Run `fsr bundle app --out /tmp/dist` in the storefront and look at what landed: `find /tmp/dist -name '*.tsx'` finds nothing, and `find /tmp/dist/serve -type d` is the static roots the boot report listed.

Read `/tmp/dist/config/bundle.toml` and compare its `[[static]]` entries against the ones in `config/app.toml`: the routes are the same and the directories are not, because the tree's are where the files landed. Then bundle `/tmp/dist` itself into `/tmp/dist2`; `diff -r /tmp/dist /tmp/dist2` comes back empty.

Now add a route to `config/app.toml`:

```toml
[[static]]
route = "/static/downloads"
dir = "downloads"
```

Make `app/downloads/` with a file in it, bundle again and watch `serve/static/downloads` appear with no other change. Then delete `app/icons/` and bundle again: `/static/icons` is gone from the output, because the host would no longer infer it either.
