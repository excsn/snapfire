---
title: FAQ · The FSR handbook
---

# Questions

## Does this site run a server?

No. Its binary renders every route to a file and exits; a static host serves the directory.

## What makes a route prerenderable?

No parameter in the pattern, and nothing on the plan reads the request, the session or an identity.

## Where does the JavaScript come from?

The bundle the build wrote, plus vendored React. Nothing is fetched at runtime.

| Route | Written to |
| :-- | :-- |
| `/` | `index.html` |
| `/faq` | `faq/index.html` |
| `/install` | `install/index.html` |
| `/tips` | `tips/index.html` |
