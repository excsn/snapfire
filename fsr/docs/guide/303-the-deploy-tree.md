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
  config/                  app.toml and the environment files
  app/
    generated/             plan.json and contracts/, read at boot
  serve/
    static/js/app/         the bundle
    static/js/vendor/      the vendored packages
    static/icons/
    static/css/
```

The process reads `config/` and `app/generated/` from its working directory. The web server reads `serve/` and can reach nothing above it. A `.tsx` file cannot be served by accident, because it is not there.

## `fsr bundle` writes it

`fsr bundle <app> [--out <dir>]` produces that tree. It defaults to `dist/` beside the project.

```
$ fsr bundle app --out dist
serve/static/js/fsr      app/../public/static/js/fsr
serve/static/js/app      app/dist
serve/static/js/vendor   app/vendor
serve/static/icons       app/icons
serve/static/css         app/styles
app/generated/plan.json  read by the host, never served
app/generated/contracts  read by the host, never served

place beside it: the binary, config/, the logging configuration
```

The routes on the left are not a list the command carries. They are the host's own static roots, read from the configuration the same way the host reads them at boot: the `[[static]]` entries the file declares, plus the four the host infers, which chapter 200 covers, `dist/` at the public path from `dist/.snapfire-build.json`, `vendor/`, `icons/` and `styles/`. So the URLs a server answers from disk are the URLs the host answered in development, and adding a `[[static]]` entry changes the deploy without anyone editing a build script. A hand written copy list is the same information written twice, and the second copy is the one that goes stale.

`server.prerender`, when configured, is copied beside the plan: those documents are read by the host and answered from the file, not served off disk.

A static root is copied whole, so whatever the directory holds is served. A vendored package that ships its `.d.ts` beside its `.js` puts those declarations under the route too. The browser never asks for them and the host would have served them in development just the same, but a directory you point a route at is a directory you have published.

## What it deliberately does not hold

The binary, `config/` and the logging configuration. Those are the three things that differ per deployment and per tool: which binary, which environment's configuration, which logging file becomes `fibre_logging.yaml`. The command names them rather than guessing, and the build script that calls it places them:

```sh
fsr bundle app --out dist

cp target/release/snapfire_www dist/
cp -R config dist/
cp fibre_logging.production.yaml dist/fibre_logging.yaml
```

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

Now add a route to `config/app.toml`:

```toml
[[static]]
route = "/static/downloads"
dir = "downloads"
```

Make `app/downloads/` with a file in it, bundle again and watch `serve/static/downloads` appear with no other change. Then delete `app/icons/` and bundle again: `/static/icons` is gone from the output, because the host would no longer infer it either.
