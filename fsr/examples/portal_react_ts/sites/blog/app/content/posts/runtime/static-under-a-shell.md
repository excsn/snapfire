---
title: Static pages under a shell that reads the session
posted: "2026-09-16"
tags: [runtime, prerender]
summary: The portal's layout reads the session for its header, so no document under /blog can be written whole. The pages are written anyway.
---

The shell's `fsr prerender` renders each blog page once and keeps the rendered subtree beside the loads. A request evaluates the layout for the visitor and splices the page in.

Running alone, the same routes are documents on disk: the index, the tag list and one file per post, from the slugs the post loader names.

## Written ahead

1. The shell lists every path a prerender writes.
2. Each subtree is rendered once, ~~per request~~ per build.
