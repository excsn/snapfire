# patterns

JSX written where a value goes, one page per pattern under `cases/`. Each case is built as an application of its own from `template/`, so a case the build refuses outright does not take the others with it.

```sh
fsr types template/app                       # the declarations the template's typecheck reads, fetched once
cargo test -p patterns                       # each case's outcome against outcomes.tsv
PATTERNS_BLESS=1 cargo test -p patterns      # takes this run's outcomes into outcomes.tsv
PATTERNS_LABEL=a cargo bench -p patterns     # results/a.tsv plus a criterion group per case
```

The bench needs `snapfirec` on `PATH` or in `SNAPFIREC` and the network once, for `react-dom/server`. For every case that builds it renders the page with the IR and with React's `renderToString` in QuickJS, over the case's own source compiled apart from the build's rewrite. A row says `same` when the two agree once React's `<!-- -->` between adjacent text, a void element's `/` and attribute order are taken out, `equivalent` when they agree once FSR's own markers are taken out of the IR's as well (an `sf-s` or `sf-i` region around what it holds, a `<template data-sf-…>` held for later, a props `<script data-sf-…>`) and `differs` otherwise. Each case that renders gets `<label>/ir/<case>` and `<label>/react/<case>` timings.

| File | What it is |
| --- | --- |
| `cases/<name>.tsx` | the page, its data written inline |
| `outcomes.tsv` | per case: `lowered`, `client:` with the causes or `refused:` with the error |
| `results/<label>.tsv` | per case from the last bench under that label: outcome, `same`, `equivalent` or `differs` against React, the bytes each wrote |
