# 301. Dependencies without npm

The question this chapter answers: how does React get into the browser when there is no `node_modules`, where do the types for it come from and what changes when a team already has its own module registry?

**For:** everyone.

## What the browser loads is committed

A browser dependency is a module the browser fetches. fsr keeps those under `app/vendor/`, committed to the repository, served from disk by the host at its conventional path and named in `app/importmap.json` so a bare specifier in a page resolves to a file the application ships. There is no install step on a checkout and nothing is fetched at runtime; the storefront's README has no `npm install` line because there is nothing for one to do.

`fsr add <app> react@18.3.1 react-dom@18.3.1/client` is how a module gets there. It fetches each package from a CDN that serves npm packages as ES modules, bundled as one file per specifier, then writes the file, the import map entry and a manifest recording what was added. A package that depends on another is bundled with it unless `--external` names the dependency, in which case the import stays bare and the dependency is vendored on its own, which is how React and React DOM share one React. A package that imports something outside its bundle that nobody named is refused with the name rather than becoming a runtime 404.

The client library itself, `@snapfire/fsr-client`, is not a package to add. It is built from `fsr/client` with snapfirec and served from that build; the storefront's import map points at it.

## Types come separately and are not committed

The editor needs declarations and the build's `tsc` step needs them too; they are not something the browser loads, so they go under `app/types/`, ignored by git. `fsr types <app>` fills it: for every vendored package it asks the npm registry for the release matching the vendored major, takes the package's own declarations when it ships them and `@types/<name>` from DefinitelyTyped when it does not, then queues whatever those declarations depend on. The generated `tsconfig.json` maps each package to its declarations, so `import { useState } from "react"` types in the editor exactly as it resolves in the browser.

The step is best effort by design. Skip it and the application still builds and runs; the editor types every import as `any` until you run it. The report's `types` section says which package's declarations came from where; `missing` names the ones it could not find with the reason.

## Why the split

Two directories because two audiences. `vendor/` is what ships and it is committed so that a checkout is complete and a deployment carries exactly what was reviewed. `types/` is what the editor reads and it is not committed because it is large, regenerable and never served. The manifests in each record the version so `fsr add` and `fsr types` know what is already there.

## When a team has xwpm

Internally, snapfire modules are published to a registry and installed with xwpm, which owns a vendor tree, an import map and a types directory of its own. fsr does not reimplement any of that. An `xwpm.wmf` in the app directory marks the application as one xwpm manages and names its layout; from then on `fsr add` and `fsr types` delegate to xwpm rather than fetching themselves, while the build reads the directories the file names. A public application never sees this path; an internal one gets the registry without the two tools drifting apart.

## The lab

Run `fsr add app dayjs@1.11.13` in the storefront. The command prints the file it wrote under `vendor/`, the import map gains a `dayjs` entry and `vendor/.fsr-vendor.json` records the version. Run `fsr types app`: the `types` section gains a row for `dayjs`, from the package's own declarations. Import it in a page and `tsc` knows its shape. Remove the entry, the file and the types directory when you are done, since the storefront does not use it.
