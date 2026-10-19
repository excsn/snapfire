# Notice

The components under `react_dev/` are the code examples of the React documentation, react.dev, copied from https://github.com/reactjs/react.dev at commit `046f17d04295ba047bb5739026b4ac3110f17028` (2026-10-06), `src/content`. They are licensed under the Creative Commons Attribution 4.0 International License (CC-BY-4.0), https://creativecommons.org/licenses/by/4.0/, by the react.dev contributors.

Each directory is one `<Sandpack>` block, named after the page it appears on and its position there. The block's files keep their authored names; `src/index.js`, which only mounts the example, is left out. Blocks whose files declare `expectedErrors` or a `package.json` are not copied. The code was taken out of the documentation's Markdown into files, each ending in a newline; nothing else in it was changed.

## svelte.dev

The components under `svelte_dev/` are the code examples of the Svelte documentation, svelte.dev, copied from https://github.com/sveltejs/svelte.dev at commit `89f941caf5a2229356fc38facae6ee0dca3122e7` (2026-10-15), `apps/svelte.dev/content`. The repository is licensed under the MIT License by the svelte.dev contributors, https://github.com/sveltejs/svelte.dev/graphs/contributors.

Each `examples-…` directory is one playground example, its `+assets` files as authored. Each `tutorial-…` directory is one exercise of the Svelte and advanced Svelte chapters as its solution reads: the `src/lib` files of `app-a` with those of `app-b` over them, an exercise with no `app-a` starting from the previous exercise's solution as svelte.dev's tutorial loader does. Exercises carrying a `package.json` are not copied. Nothing in the files was changed.

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## vuejs.org

The components under `vuejs_org/` are the code examples of the Vue documentation, vuejs.org, built from https://github.com/vuejs/docs at commit `40aa88af0094f7bab4aaf786e55c748a6a251d88` (2026-09-15), `src/examples/src` and `src/tutorial/src`. They are licensed under the Creative Commons Attribution 4.0 International License (CC-BY-4.0), https://creativecommons.org/licenses/by/4.0/, by Evan You and the Vue documentation contributors.

The documentation keeps each component as a template, a Composition API script, an Options API script and a style. Each directory here is one example or one tutorial step as the documentation shows it as single-file components: assembled by a port of `resolveSFCExample` in `src/examples/utils.ts`, once with Composition API scripts as `<script setup>` (`-composition`) and once with Options API scripts (`-options`). A tutorial step takes its `_hint` files, the step's solution, over its own. Nothing else was changed.
