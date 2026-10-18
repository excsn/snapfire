---
title: Drain before stop
posted: "2026-10-17"
tags: [runtime]
summary: A host that does not own its listener still answers its readiness probe with 503 once it is told to stop.
---

A stop signal starts a drain. The host calls `begin_drain` and its readiness probe answers 503 from then on, so a load balancer takes it out of rotation while requests already in flight finish.

This post sits two directories under `content/posts` in `runtime/host/`. The collection lists it like the others.
