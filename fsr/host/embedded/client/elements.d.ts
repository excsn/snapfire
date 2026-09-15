/**
* The shadow root of a custom element whose template the server wrote. The
* HTML parser attaches a declarative shadow root itself. Markup that arrived
* through `innerHTML`, an htmx swap among it, keeps the
* `<template shadowrootmode>` as a child instead, so the root is attached
* with the mode and options the template carries and the template taken out.
* A closed root the parser attached is
* reached through `internals`. `null` when the element has neither.
*/
export declare function shadowOf(element: HTMLElement, internals?: ElementInternals): ShadowRoot | null;
